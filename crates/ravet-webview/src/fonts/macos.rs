use super::{
    sfnt::{FontTable, assemble, valid_tag},
    *,
};
use std::{
    cell::Cell,
    ffi::{CStr, c_char, c_void},
    marker::PhantomData,
    ptr::NonNull,
    rc::Rc,
};
#[repr(C)]
struct NativeDescription {
    family: [c_char; 1025],
    style: [c_char; 1025],
    display: [c_char; 1025],
    postscript: [c_char; 1025],
    weight: f64,
    width: f64,
    slant: f64,
    traits: u32,
    availability: u32,
    format: u32,
}
impl Default for NativeDescription {
    fn default() -> Self {
        Self {
            family: [0; 1025],
            style: [0; 1025],
            display: [0; 1025],
            postscript: [0; 1025],
            weight: 0.0,
            width: 0.0,
            slant: 0.0,
            traits: 0,
            availability: 0,
            format: 0,
        }
    }
}
#[repr(C)]
#[derive(Default)]
struct NativeAxis {
    tag: u32,
    hidden: u32,
    minimum: f64,
    default_value: f64,
    maximum: f64,
    value: f64,
}
unsafe extern "C" {
    fn ravet_fonts_create(destination: *mut *mut c_void, count: *mut usize) -> u32;
    fn ravet_fonts_descriptor(
        owner: *mut c_void,
        index: usize,
        destination: *mut NativeDescription,
    ) -> u32;
    fn ravet_fonts_load(
        owner: *mut c_void,
        index: usize,
        destination: *mut *mut c_void,
        tables: *mut usize,
        axes: *mut usize,
    ) -> u32;
    fn ravet_fonts_table(
        resource: *mut c_void,
        index: usize,
        tag: *mut u32,
        length: *mut usize,
    ) -> u32;
    fn ravet_fonts_copy_table(
        resource: *mut c_void,
        index: usize,
        destination: *mut u8,
        capacity: usize,
    ) -> u32;
    fn ravet_fonts_axis(resource: *mut c_void, index: usize, destination: *mut NativeAxis) -> u32;
    fn ravet_fonts_resource_destroy(resource: *mut c_void);
    fn ravet_fonts_destroy(owner: *mut c_void);
}
fn status(value: u32) -> Result<(), FontError> {
    match value {
        0 => Ok(()),
        1 => Err(FontError::InvalidInput),
        2 => Err(FontError::Capacity),
        3 => Err(FontError::ForeignSelection),
        4 => Err(FontError::WrongThread),
        5 => Err(FontError::Unavailable),
        6 => Err(FontError::UnsupportedFont),
        _ => Err(FontError::NativeFailure),
    }
}
fn name(bytes: &[c_char; 1025]) -> Result<&str, FontError> {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .ok_or(FontError::NativeFailure)?;
    // SAFETY: checked first NUL within the entire owned fixed array; no borrowed native buffer.
    let bytes = unsafe { std::slice::from_raw_parts(bytes.as_ptr().cast::<u8>(), end + 1) };
    CStr::from_bytes_with_nul(bytes)
        .map_err(|_| FontError::NativeFailure)?
        .to_str()
        .map_err(|_| FontError::UnsupportedFont)
}
struct NativeOwner(NonNull<c_void>);
impl Drop for NativeOwner {
    fn drop(&mut self) {
        // SAFETY: this temporary and every containing owner is local main-thread-only.
        unsafe {
            ravet_fonts_destroy(self.0.as_ptr());
        }
    }
}
struct NativeResource(NonNull<c_void>);
impl Drop for NativeResource {
    fn drop(&mut self) {
        // SAFETY: private synchronous local resource with no foreign borrowed lifetime.
        unsafe {
            ravet_fonts_resource_destroy(self.0.as_ptr());
        }
    }
}
struct Operation<'a>(&'a Cell<bool>);
impl Drop for Operation<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
/// Immutable main-thread CoreText snapshot. Loading never registers/downloads fonts,
/// creates a window, or silently substitutes an unsupported requested style.
///
/// ```compile_fail
/// # use ravet_webview::fonts::SystemFontCatalog;
/// fn transfer(catalog:SystemFontCatalog) { std::thread::spawn(move || drop(catalog)); }
/// ```
pub struct SystemFontCatalog {
    native: NativeOwner,
    catalog: FontCatalog,
    busy: Cell<bool>,
    _main: PhantomData<Rc<()>>,
}
impl SystemFontCatalog {
    /// Snapshot <=4096 application-available descriptors, <=4MiB copied names.
    /// CoreText's transient allocation/time is not bounded by these output limits.
    pub fn discover() -> Result<Self, FontError> {
        if !crate::is_main_thread() {
            return Err(FontError::WrongThread);
        }
        let mut raw = std::ptr::null_mut();
        let mut count = 0;
        // SAFETY: two disjoint local outputs; native publishes a retained complete owner only on success.
        status(unsafe { ravet_fonts_create(&mut raw, &mut count) })?;
        let native = NativeOwner(NonNull::new(raw).ok_or(FontError::NativeFailure)?);
        if count > MAXIMUM_FONT_ENTRIES {
            return Err(FontError::Capacity);
        }
        let mut descriptors = Vec::with_capacity(count);
        let mut bytes = 0usize;
        for index in 0..count {
            let mut d = NativeDescription::default();
            // SAFETY: live retained owner and exact copied structure; index bounded by its published count.
            status(unsafe { ravet_fonts_descriptor(native.0.as_ptr(), index, &mut d) })?;
            if d.availability & !15 != 0 {
                return Err(FontError::NativeFailure);
            }
            let traits = FontTraits {
                weight: d.weight,
                width: d.width,
                slant: d.slant,
                italic: d.traits & 1 != 0,
                bold: d.traits & 2 != 0,
                expanded: d.traits & 32 != 0,
                condensed: d.traits & 64 != 0,
                monospace: d.traits & 1024 != 0,
            };
            let mut descriptor = FontDescriptor::new(
                name(&d.family)?,
                name(&d.style)?,
                name(&d.display)?,
                name(&d.postscript)?,
                if traits.valid() {
                    traits
                } else {
                    FontTraits::default()
                },
                FontAvailability {
                    enabled: d.availability & 1 != 0,
                    downloadable: d.availability & 2 != 0,
                    downloaded: d.availability & 4 != 0,
                },
            )?;
            if !traits.valid()
                || !(matches!(d.format, 2 | 3)
                    || cfg!(feature = "postscript-fonts") && d.format == 1)
                || d.availability & 8 != 0
            {
                descriptor =
                    descriptor.native_unsupported(!traits.valid(), FontError::UnsupportedFont);
            }
            bytes = bytes
                .checked_add(descriptor.metadata_bytes())
                .ok_or(FontError::Capacity)?;
            if bytes > MAXIMUM_FONT_METADATA_BYTES {
                return Err(FontError::Capacity);
            }
            descriptors.push(descriptor);
        }
        Ok(Self {
            native,
            catalog: FontCatalog::new(descriptors)?,
            busy: Cell::new(false),
            _main: PhantomData,
        })
    }
    pub fn catalog(&self) -> &FontCatalog {
        &self.catalog
    }
    /// Copy the exact selected face into an independent checked SFNT.
    /// Static/variable glyf styles; optional postscript-fonts also admits exact
    /// OpenType PostScript CFF/CFF2 styles as OTTO. Unavailable/downloadable and
    /// synthetic effects reject. No system bytes are written to disk or delivered to HTML.
    /// f64 native axes must convert to in-range f32 within1/65536 user units;
    /// backend normalized rounding remains separately visible in TextFont. Native
    /// min/max metadata may differ <=1e-4 from authoritative fvar bounds; exact
    /// source/default/hidden and selected-coordinate admission remain strict.
    pub fn load(&self, selection: FontSelection) -> Result<LoadedSystemFont, FontError> {
        if !crate::is_main_thread() {
            return Err(FontError::WrongThread);
        }
        let index = self.catalog.source_index(selection)?;
        if self.busy.replace(true) {
            return Err(FontError::Unavailable);
        }
        let _operation = Operation(&self.busy);
        let descriptor = self.catalog.descriptor(selection)?;
        if let Some(error) = descriptor.diagnostic {
            return Err(error);
        }
        if !descriptor.availability.enabled || descriptor.availability.downloadable {
            return Err(FontError::Unavailable);
        }
        let mut raw = std::ptr::null_mut();
        let mut table_count = 0;
        let mut axis_count = 0;
        // SAFETY: retained main-thread owner, three disjoint local outputs, no callbacks/pointer escapes.
        status(unsafe {
            ravet_fonts_load(
                self.native.0.as_ptr(),
                index,
                &mut raw,
                &mut table_count,
                &mut axis_count,
            )
        })?;
        let resource = NativeResource(NonNull::new(raw).ok_or(FontError::NativeFailure)?);
        if table_count > MAXIMUM_FONT_TABLES || axis_count > MAXIMUM_FONT_AXES {
            return Err(FontError::Capacity);
        }
        let mut tables = Vec::with_capacity(table_count);
        let mut total = 0usize;
        for index in 0..table_count {
            let mut tag = 0;
            let mut len = 0;
            // SAFETY: resource owns count/table buffers throughout this synchronous scope.
            status(unsafe { ravet_fonts_table(resource.0.as_ptr(), index, &mut tag, &mut len) })?;
            total = total.checked_add(len).ok_or(FontError::Capacity)?;
            if total > MAXIMUM_SYSTEM_FONT_BYTES {
                return Err(FontError::Capacity);
            }
            let mut bytes = vec![0; len];
            // SAFETY: unique destination exact declared capacity; zero length requires no dereference.
            status(unsafe {
                ravet_fonts_copy_table(resource.0.as_ptr(), index, bytes.as_mut_ptr(), bytes.len())
            })?;
            tables.push(FontTable {
                tag: tag.to_be_bytes(),
                bytes,
            });
        }
        let raw_axes = font_axes(&tables)?;
        if raw_axes.len() != axis_count {
            return Err(FontError::UnsupportedFont);
        }
        let mut variations = Vec::with_capacity(axis_count);
        let mut axes = Vec::with_capacity(axis_count);
        for index in 0..axis_count {
            let mut axis = NativeAxis::default();
            // SAFETY: copied exact ABI object from retained private resource at admitted index.
            status(unsafe { ravet_fonts_axis(resource.0.as_ptr(), index, &mut axis) })?;
            let tag = axis.tag.to_be_bytes();
            let raw = raw_axes
                .iter()
                .find(|a| a.0 == tag)
                .ok_or(FontError::UnsupportedFont)?;
            if !valid_tag(tag)
                || axis.hidden > 1
                || variations.iter().any(|v: &FontVariation| v.tag == tag)
                || ![axis.minimum, axis.default_value, axis.maximum, axis.value]
                    .iter()
                    .all(|v| v.is_finite())
                || axis.default_value != raw.2
                || axis.hidden != raw.4
                || (axis.minimum - raw.1).abs() > 1e-4
                || (axis.maximum - raw.3).abs() > 1e-4
                || axis.value < raw.1
                || axis.value > raw.3
            {
                return Err(FontError::UnsupportedFont);
            }
            let value = axis.value as f32;
            if !value.is_finite()
                || (value as f64) < raw.1
                || (value as f64) > raw.3
                || ((value as f64) - axis.value).abs() > 1.0 / 65536.0
            {
                return Err(FontError::UnsupportedFont);
            }
            variations.push(FontVariation { tag, value });
            axes.push(SystemFontAxis {
                tag,
                minimum: raw.1,
                default: raw.2,
                maximum: raw.3,
                native_minimum: axis.minimum,
                native_default: axis.default_value,
                native_maximum: axis.maximum,
                native_value: axis.value,
                hidden: axis.hidden != 0,
            });
        }
        let bytes = assemble(tables)?;
        Ok(LoadedSystemFont {
            descriptor: descriptor.clone(),
            selection,
            bytes,
            variations,
            axes,
        })
    }
}
// Validate exact source fvar axis bounds against CoreText. Full backend parsing remains separate.
type Axis = ([u8; 4], f64, f64, f64, u32);
fn font_axes(tables: &[FontTable]) -> Result<Vec<Axis>, FontError> {
    let Some(table) = tables.iter().find(|t| t.tag == *b"fvar") else {
        return Ok(Vec::new());
    };
    let data = &table.bytes;
    let u16_at = |i: usize| -> Result<u16, FontError> {
        Ok(u16::from_be_bytes(
            data.get(i..i + 2)
                .ok_or(FontError::UnsupportedFont)?
                .try_into()
                .unwrap(),
        ))
    };
    if u16_at(0)? != 1 || u16_at(2)? != 0 || u16_at(6)? != 2 {
        return Err(FontError::UnsupportedFont);
    }
    let offset = u16_at(4)? as usize;
    let count = u16_at(8)? as usize;
    let size = u16_at(10)? as usize;
    if count > MAXIMUM_FONT_AXES {
        return Err(FontError::Capacity);
    }
    if offset < 16
        || size != 20
        || offset
            .checked_add(count.checked_mul(size).ok_or(FontError::Capacity)?)
            .ok_or(FontError::Capacity)?
            > data.len()
    {
        return Err(FontError::UnsupportedFont);
    }
    let mut result = Vec::with_capacity(count);
    for index in 0..count {
        let start = offset + index * size;
        let axis = &data[start..start + 20];
        let fixed =
            |i: usize| i32::from_be_bytes(axis[i..i + 4].try_into().unwrap()) as f64 / 65536.0;
        let tag: [u8; 4] = axis[..4].try_into().unwrap();
        let min = fixed(4);
        let default = fixed(8);
        let max = fixed(12);
        let flags = u16::from_be_bytes(axis[16..18].try_into().unwrap());
        if !valid_tag(tag)
            || result.iter().any(|a: &Axis| a.0 == tag)
            || min > default
            || default > max
            || flags & !1 != 0
        {
            return Err(FontError::UnsupportedFont);
        }
        result.push((tag, min, default, max, (flags & 1) as u32));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copied_source_axis_bounds_tags_flags_and_whole_parse_failures() {
        fn fvar() -> Vec<u8> {
            let mut d = vec![0u8; 36];
            d[0..2].copy_from_slice(&1u16.to_be_bytes());
            d[4..6].copy_from_slice(&16u16.to_be_bytes());
            d[6..8].copy_from_slice(&2u16.to_be_bytes());
            d[8..10].copy_from_slice(&1u16.to_be_bytes());
            d[10..12].copy_from_slice(&20u16.to_be_bytes());
            d[16..20].copy_from_slice(b"wght");
            for (i, n) in [(20, 100i32), (24, 400), (28, 900)] {
                d[i..i + 4].copy_from_slice(&(n * 65536).to_be_bytes())
            }
            d
        }
        let source = fvar();
        let tables = |bytes| {
            vec![FontTable {
                tag: *b"fvar",
                bytes,
            }]
        };
        assert_eq!(
            font_axes(&tables(source.clone())).unwrap(),
            vec![(*b"wght", 100., 400., 900., 0)]
        );
        for kind in 0..9 {
            let mut data = source.clone();
            match kind {
                0 => data.truncate(35),
                1 => data[28..32].copy_from_slice(&0i32.to_be_bytes()),
                2 => data[32..34].copy_from_slice(&2u16.to_be_bytes()),
                3 => data[16..20].copy_from_slice(b"wg t"),
                4 => data[4..6].copy_from_slice(&0u16.to_be_bytes()),
                5 => data[0..2].copy_from_slice(&2u16.to_be_bytes()),
                6 => data[2..4].copy_from_slice(&1u16.to_be_bytes()),
                7 => data[6..8].copy_from_slice(&0u16.to_be_bytes()),
                _ => data[10..12].copy_from_slice(&21u16.to_be_bytes()),
            }
            assert_eq!(font_axes(&tables(data)), Err(FontError::UnsupportedFont));
        }
        let mut data = source.clone();
        data[8..10].copy_from_slice(&33u16.to_be_bytes());
        assert_eq!(font_axes(&tables(data)), Err(FontError::Capacity));
        let mut data = source;
        data.extend_from_within(16..36);
        data[8..10].copy_from_slice(&2u16.to_be_bytes());
        assert_eq!(font_axes(&tables(data)), Err(FontError::UnsupportedFont));
        assert!(font_axes(&[]).unwrap().is_empty());
    }
    #[test]
    fn native_strings_status_and_private_unsupported_metadata_remain_typed() {
        let mut raw = [0; 1025];
        raw[..3].copy_from_slice(&[97, 98, 99]);
        assert_eq!(name(&raw), Ok("abc"));
        assert_eq!(name(&[1; 1025]), Err(FontError::NativeFailure));
        raw[0] = -1;
        assert_eq!(name(&raw), Err(FontError::UnsupportedFont));
        assert_eq!(status(7), Err(FontError::NativeFailure));
        assert_eq!(status(999), Err(FontError::NativeFailure));
        let d = FontDescriptor::new(
            "A",
            "Bold",
            "A Bold",
            "A-Bold",
            FontTraits::default(),
            FontAvailability {
                enabled: true,
                ..Default::default()
            },
        )
        .unwrap()
        .native_unsupported(true, FontError::UnsupportedFont);
        assert_eq!(d.traits(), None);
        assert_eq!(d.diagnostic(), Some(FontError::UnsupportedFont));
        assert_eq!(d.postscript_name(), "A-Bold");
        assert_eq!(
            FontDescriptor::new(
                "A",
                "",
                "",
                "A",
                FontTraits {
                    slant: -97.,
                    ..Default::default()
                },
                FontAvailability::default()
            ),
            Err(FontError::InvalidInput)
        );
        let busy = Cell::new(true);
        assert!(busy.replace(true));
        assert!(busy.get());
        {
            let _operation = Operation(&busy);
        }
        assert!(!busy.get());
    }
}
