//! Narrow OS queries. No hashing, licensing policy or process execution.
use std::{ffi::c_void, path::PathBuf};

/// Find the loaded module containing an address without dereferencing it.
pub fn module_path(address: *const c_void) -> Option<PathBuf> {
    if address.is_null() {
        return None;
    }
    backend::module_path(address)
}
/// Raw platform UUID on macOS. Other platforms return None.
pub fn hardware_uuid() -> Option<String> {
    backend::hardware_uuid()
}
/// Raw SMBIOS firmware on Windows (at most 1 MiB); other platforms return None.
pub fn smbios_firmware() -> Option<Vec<u8>> {
    backend::smbios_firmware()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn module_lookup_uses_addresses_without_dereferencing_them() {
        assert!(module_path(std::ptr::null()).is_none());
        assert!(module_path(module_path as *const c_void).is_some());
        assert!(module_path(std::ptr::dangling::<c_void>()).is_none());
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn platform_uuid_is_raw_and_contains_no_product_hashing() {
        let uuid = hardware_uuid().expect("macOS platform expert UUID");
        assert!(uuid.len() == 36 && uuid.chars().all(|c| c == '-' || c.is_ascii_hexdigit()));
        assert!(smbios_firmware().is_none());
    }
}

#[cfg(unix)]
#[allow(unsafe_code)]
mod backend {
    use super::*;
    use std::{
        ffi::{CStr, c_char},
        os::unix::ffi::OsStrExt,
    };
    #[repr(C)]
    struct DlInfo {
        filename: *const c_char,
        base: *mut c_void,
        symbol: *const c_char,
        symbol_address: *mut c_void,
    }
    #[cfg_attr(target_os = "linux", link(name = "dl"))]
    unsafe extern "C" {
        fn dladdr(address: *const c_void, info: *mut DlInfo) -> i32;
    }
    pub fn module_path(address: *const c_void) -> Option<PathBuf> {
        let mut info = DlInfo {
            filename: std::ptr::null(),
            base: std::ptr::null_mut(),
            symbol: std::ptr::null(),
            symbol_address: std::ptr::null_mut(),
        };
        // OS supplies a live terminated module filename on success.
        if unsafe { dladdr(address, &mut info) } == 0 || info.filename.is_null() {
            return None;
        }
        let bytes = unsafe { CStr::from_ptr(info.filename) }.to_bytes();
        Some(PathBuf::from(std::ffi::OsStr::from_bytes(bytes)))
    }
    pub fn smbios_firmware() -> Option<Vec<u8>> {
        None
    }
    #[cfg(not(target_os = "macos"))]
    pub fn hardware_uuid() -> Option<String> {
        None
    }
    #[cfg(target_os = "macos")]
    pub fn hardware_uuid() -> Option<String> {
        const UTF8: u32 = 0x08000100;
        #[link(name = "IOKit", kind = "framework")]
        unsafe extern "C" {
            fn IOServiceMatching(name: *const c_char) -> *mut c_void;
            fn IOServiceGetMatchingService(port: u32, matching: *mut c_void) -> u32;
            fn IORegistryEntryCreateCFProperty(
                entry: u32,
                key: *const c_void,
                allocator: *const c_void,
                options: u32,
            ) -> *const c_void;
            fn IOObjectRelease(object: u32) -> i32;
        }
        #[link(name = "CoreFoundation", kind = "framework")]
        unsafe extern "C" {
            fn CFStringCreateWithCString(
                allocator: *const c_void,
                text: *const c_char,
                encoding: u32,
            ) -> *const c_void;
            fn CFGetTypeID(value: *const c_void) -> usize;
            fn CFStringGetTypeID() -> usize;
            fn CFStringGetCString(
                value: *const c_void,
                buffer: *mut c_char,
                capacity: isize,
                encoding: u32,
            ) -> bool;
            fn CFRelease(value: *const c_void);
        }
        // Matching dictionary is consumed by IOServiceGetMatchingService; all
        // acquired handles/properties/strings below are released on every path.
        unsafe {
            let matching = IOServiceMatching(c"IOPlatformExpertDevice".as_ptr());
            if matching.is_null() {
                return None;
            }
            let service = IOServiceGetMatchingService(0, matching);
            if service == 0 {
                return None;
            }
            let key = CFStringCreateWithCString(std::ptr::null(), c"IOPlatformUUID".as_ptr(), UTF8);
            if key.is_null() {
                IOObjectRelease(service);
                return None;
            }
            let value = IORegistryEntryCreateCFProperty(service, key, std::ptr::null(), 0);
            CFRelease(key);
            IOObjectRelease(service);
            if value.is_null() {
                return None;
            }
            let mut buffer = [0_i8; 4096];
            let result = if CFGetTypeID(value) == CFStringGetTypeID()
                && CFStringGetCString(value, buffer.as_mut_ptr(), buffer.len() as isize, UTF8)
            {
                CStr::from_ptr(buffer.as_ptr())
                    .to_str()
                    .ok()
                    .map(str::to_owned)
            } else {
                None
            };
            CFRelease(value);
            result
        }
    }
}
#[cfg(windows)]
#[allow(unsafe_code)]
mod backend {
    use super::*;
    use std::os::windows::ffi::OsStringExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetModuleHandleExW(flags: u32, address: *const u16, module: *mut *mut c_void) -> i32;
        fn GetModuleFileNameW(module: *mut c_void, buffer: *mut u16, capacity: u32) -> u32;
        fn GetSystemFirmwareTable(provider: u32, table: u32, buffer: *mut u8, capacity: u32)
        -> u32;
    }
    pub fn module_path(address: *const c_void) -> Option<PathBuf> {
        let mut module = std::ptr::null_mut();
        let mut path = vec![0_u16; 32768];
        // FROM_ADDRESS means address is treated as a code/data address, never a string.
        unsafe {
            if GetModuleHandleExW(0x4 | 0x2, address.cast(), &mut module) == 0 {
                return None;
            }
            let n = GetModuleFileNameW(module, path.as_mut_ptr(), path.len() as u32) as usize;
            if n == 0 || n >= path.len() {
                return None;
            }
            Some(PathBuf::from(std::ffi::OsString::from_wide(&path[..n])))
        }
    }
    pub fn hardware_uuid() -> Option<String> {
        None
    }
    pub fn smbios_firmware() -> Option<Vec<u8>> {
        unsafe {
            let size = GetSystemFirmwareTable(0x52534d42, 0, std::ptr::null_mut(), 0);
            if size == 0 || size > 1048576 {
                return None;
            }
            let mut bytes = vec![0; size as usize];
            if GetSystemFirmwareTable(0x52534d42, 0, bytes.as_mut_ptr(), size) != size {
                return None;
            }
            Some(bytes)
        }
    }
}
#[cfg(not(any(unix, windows)))]
mod backend {
    use super::*;
    pub fn module_path(_: *const c_void) -> Option<PathBuf> {
        None
    }
    pub fn hardware_uuid() -> Option<String> {
        None
    }
    pub fn smbios_firmware() -> Option<Vec<u8>> {
        None
    }
}
