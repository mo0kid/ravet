//! Explicit caller-linked trusted VST3 catalogue and scoped instance owners.
//! Cargo supplies the Rust ABI/owner only; native provider compilation and linking
//! require an explicitly supplied external SDK. See docs/vst3-hosting.md.
#![allow(unsafe_code)]
use std::{
    collections::BTreeSet, ffi::c_void, marker::PhantomData, path::Path, ptr::NonNull, rc::Rc,
};

#[repr(C)]
pub struct FactoryRecord {
    pub struct_size: u32,
    pub flags: i32,
    pub class_count: u32,
    pub host_context_result: i32,
    pub vendor: [u8; 64],
    pub url: [u8; 256],
    pub email: [u8; 128],
}
#[repr(C)]
pub struct ClassRecord {
    pub struct_size: u32,
    pub id: [u8; 16],
    pub cardinality: i32,
    pub class_flags: u32,
    pub vendor_from_factory: u32,
    pub category: [u8; 32],
    pub name: [u8; 192],
    pub vendor: [u8; 192],
    pub version: [u8; 192],
    pub sdk_version: [u8; 192],
    pub subcategories: [u8; 128],
}
/// Caller-linked provider v1 C ABI. Nullable function fields reject before calls.
#[repr(C)]
pub struct RawProvider {
    pub struct_size: u32,
    pub version: u32,
    pub open: Option<unsafe extern "C" fn(*const u8, u32, *mut *mut c_void, *mut i32) -> i32>,
    pub factory: Option<unsafe extern "C" fn(*const c_void, *mut FactoryRecord) -> i32>,
    pub class_info: Option<unsafe extern "C" fn(*const c_void, u32, *mut ClassRecord) -> i32>,
    pub close: Option<unsafe extern "C" fn(*mut c_void, *mut i32) -> i32>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    ProviderAbi,
    InvalidPath,
    Capacity,
    Metadata,
    Native { status: i32, result: i32 },
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "VST3 catalogue error: {self:?}")
    }
}
impl std::error::Error for Error {}
/// Explicit consuming retirement outcome; both variants prove ownership retired.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Retirement {
    Complete { context_result: i32 },
    FailedNative { context_result: i32 },
}
/// A rejected retirement retains the exact owner for explicit retry/recovery.
pub struct RetireFailure<'a> {
    pub owner: Library<'a>,
    pub error: Error,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassMetadata {
    pub id: [u8; 16],
    pub cardinality: i32,
    pub flags: u32,
    pub category: String,
    pub name: String,
    pub vendor: String,
    pub version: String,
    pub sdk_version: String,
    pub subcategories: String,
    pub vendor_from_factory: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FactoryMetadata {
    pub flags: i32,
    /// Exact optional native factory context result (including NotImplemented).
    pub host_context_result: i32,
    pub vendor: String,
    pub url: String,
    pub email: String,
}
/// Verified borrowed provider code/table lifetime, restricted to one owner thread.
#[derive(Clone, Copy)]
pub struct Provider<'a> {
    raw: &'a RawProvider,
    thread: PhantomData<Rc<()>>,
}
impl<'a> Provider<'a> {
    /// Establish explicit native provider trust and main-thread ownership.
    ///
    /// # Safety
    /// The table and executable provider library must remain live for `'a`,
    /// implement the exact C ABI and pointer/ownership contracts, contain C++
    /// exceptions, and remain callable on its required control thread. The SDK
    /// provider requires the platform main thread; fixtures can supply an explicit
    /// owner-thread provider. Caller must currently run on the required thread.
    /// No sandbox or authentication
    /// is supplied; a table reference alone does not pin a dynamically loaded DLL.
    pub unsafe fn from_raw(raw: &'a RawProvider) -> Result<Self, Error> {
        if raw.struct_size as usize != std::mem::size_of::<RawProvider>()
            || raw.version != 1
            || raw.open.is_none()
            || raw.factory.is_none()
            || raw.class_info.is_none()
            || raw.close.is_none()
        {
            return Err(Error::ProviderAbi);
        }
        Ok(Self {
            raw,
            thread: PhantomData,
        })
    }
    /// Load exactly the selected native module and stage its complete catalogue.
    /// No system scan, implicit signature trust, instance or factory escape occurs.
    ///
    /// # Safety
    /// Caller explicitly trusts this module and its dependencies as compliant VST3
    /// code with valid native pointers/lifetimes/threading, including loader and
    /// factory retirement. Machine code has unrestricted process access; no crash
    /// timeout, isolation, allocator/RSS limit or real-time deadline is provided.
    pub unsafe fn load(self, path: impl AsRef<Path>) -> Result<Library<'a>, Error> {
        let path = path.as_ref().to_str().ok_or(Error::InvalidPath)?;
        if path.is_empty() || path.len() > 4096 || path.as_bytes().contains(&0) {
            return Err(Error::InvalidPath);
        }
        let mut handle = std::ptr::null_mut();
        let mut native = 0;
        // SAFETY: verified provider lifetime/contracts and explicit module trust;
        // bounded borrowed UTF8 path and exclusive valid output cells.
        let status = unsafe {
            self.raw.open.unwrap()(path.as_ptr(), path.len() as u32, &mut handle, &mut native)
        };
        if status != 0 {
            return Err(Error::Native {
                status,
                result: native,
            });
        }
        let handle = NonNull::new(handle).ok_or(Error::Metadata)?;
        let mut owner = Library {
            provider: self,
            handle: Some(handle),
            factory: None,
            classes: Vec::new(),
        };
        let mut record = FactoryRecord {
            struct_size: std::mem::size_of::<FactoryRecord>() as u32,
            flags: 0,
            class_count: 0,
            host_context_result: 0,
            vendor: [0; 64],
            url: [0; 256],
            email: [0; 128],
        };
        // SAFETY: newly owned live provider handle and exact exclusive output record.
        let status = unsafe { self.raw.factory.unwrap()(handle.as_ptr(), &mut record) };
        if status != 0 {
            return Err(Error::Native { status, result: 0 });
        }
        if record.struct_size as usize != std::mem::size_of::<FactoryRecord>() {
            return Err(Error::Metadata);
        }
        if !(1..=256).contains(&record.class_count) {
            return Err(Error::Capacity);
        }
        let factory = FactoryMetadata {
            flags: record.flags,
            host_context_result: record.host_context_result,
            vendor: text(&record.vendor, true)?,
            url: text(&record.url, false)?,
            email: text(&record.email, false)?,
        };
        let mut classes = Vec::with_capacity(record.class_count as usize);
        let mut ids = BTreeSet::new();
        for index in 0..record.class_count {
            let mut item = ClassRecord {
                struct_size: std::mem::size_of::<ClassRecord>() as u32,
                id: [0; 16],
                cardinality: 0,
                class_flags: 0,
                vendor_from_factory: 0,
                category: [0; 32],
                name: [0; 192],
                vendor: [0; 192],
                version: [0; 192],
                sdk_version: [0; 192],
                subcategories: [0; 128],
            };
            // SAFETY: bounded native index, exact exclusive record and live owner.
            let status = unsafe { self.raw.class_info.unwrap()(handle.as_ptr(), index, &mut item) };
            if status != 0 {
                return Err(Error::Native { status, result: 0 });
            }
            if item.struct_size as usize != std::mem::size_of::<ClassRecord>()
                || item.id == [0; 16]
                || !ids.insert(item.id)
                || item.cardinality <= 0
                || item.vendor_from_factory > 1
            {
                return Err(Error::Metadata);
            }
            classes.push(ClassMetadata {
                id: item.id,
                cardinality: item.cardinality,
                flags: item.class_flags,
                category: text(&item.category, true)?,
                name: text(&item.name, true)?,
                vendor: text(&item.vendor, true)?,
                version: text(&item.version, false)?,
                sdk_version: text(&item.sdk_version, false)?,
                subcategories: text(&item.subcategories, false)?,
                vendor_from_factory: item.vendor_from_factory != 0,
            });
        }
        owner.factory = Some(factory);
        owner.classes = classes;
        Ok(owner)
    }
}
fn text(bytes: &[u8], required: bool) -> Result<String, Error> {
    let n = bytes
        .iter()
        .position(|byte| *byte == 0)
        .ok_or(Error::Metadata)?;
    if required && n == 0 {
        return Err(Error::Metadata);
    }
    Ok(std::str::from_utf8(&bytes[..n])
        .map_err(|_| Error::Metadata)?
        .to_owned())
}
/// Main-thread native owner and immutable staged catalogue. Metadata getters
/// execute no plug-in code. Drop performs native retirement outside audio; use
/// explicit retire to observe the optional context-clear result and failures.
///
/// ```compile_fail
/// fn require_send<T: Send>() {}
/// require_send::<ravet_host::vst3::Library<'static>>();
/// ```
/// ```compile_fail
/// fn require_sync<T: Sync>() {}
/// require_sync::<ravet_host::vst3::Library<'static>>();
/// ```
pub struct Library<'a> {
    provider: Provider<'a>,
    handle: Option<NonNull<c_void>>,
    factory: Option<FactoryMetadata>,
    classes: Vec<ClassMetadata>,
}
impl<'a> Library<'a> {
    pub fn factory(&self) -> &FactoryMetadata {
        self.factory.as_ref().expect("complete catalogue")
    }
    pub fn classes(&self) -> &[ClassMetadata] {
        &self.classes
    }
    pub fn find(&self, id: &[u8; 16]) -> Option<&ClassMetadata> {
        self.classes.iter().find(|item| &item.id == id)
    }
    /// Consume native ownership and report the original context-clear result.
    /// NotImplemented is an explicitly optional outcome. Other native clear
    /// failures still consume the retired owner and never cause automatic retry.
    pub fn retire(mut self) -> Result<Retirement, Box<RetireFailure<'a>>> {
        let handle = self.handle.take().expect("live owner");
        let mut native = 0;
        // SAFETY: verified provider/current main thread and exclusive live handle.
        let status = unsafe { self.provider.raw.close.unwrap()(handle.as_ptr(), &mut native) };
        match status {
            0 => Ok(Retirement::Complete {
                context_result: native,
            }),
            7 => Ok(Retirement::FailedNative {
                context_result: native,
            }),
            _ => {
                self.handle = Some(handle);
                Err(Box::new(RetireFailure {
                    owner: self,
                    error: Error::Native {
                        status,
                        result: native,
                    },
                }))
            }
        }
    }
}
impl Drop for Library<'_> {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let mut ignored = 0;
            // SAFETY: !Send/Sync retains verified owner thread/provider lifetime;
            // exclusive owned handle is retired exactly once without retry.
            let _ = unsafe { self.provider.raw.close.unwrap()(handle.as_ptr(), &mut ignored) };
        }
    }
}

#[cfg(test)]
#[path = "vst3_tests.rs"]
mod tests;

/// Independent ABI-v2 instance ownership and scoped main-bus f32 audio.
#[path = "vst3/instances.rs"]
pub mod instances;
