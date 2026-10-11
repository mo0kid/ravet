//! Explicit copied font catalogues and owned system-font resources.
//!
//! Discovery/loading is opt-in macOS UI-thread work, never an audio callback.
//! A catalogue selection belongs to one immutable process-local snapshot. Loaded
//! bytes are independent of that owner; redistribution remains consumer-owned.
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod macos;
#[cfg(any(target_os = "macos", test))]
mod sfnt;
#[cfg(target_os = "macos")]
pub use macos::SystemFontCatalog;

pub const MAXIMUM_FONT_ENTRIES: usize = 4096;
pub const MAXIMUM_FONT_NAME_BYTES: usize = 1024;
pub const MAXIMUM_FONT_METADATA_BYTES: usize = 4 * 1024 * 1024;
pub const MAXIMUM_SYSTEM_FONT_BYTES: usize = 16 * 1024 * 1024;
pub const MAXIMUM_FONT_TABLES: usize = 256;
pub const MAXIMUM_FONT_AXES: usize = 32;

/// Whole-operation admission and platform resource failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontError {
    InvalidInput,
    Capacity,
    ForeignSelection,
    WrongThread,
    Unavailable,
    UnsupportedFont,
    NativeFailure,
}
impl std::fmt::Display for FontError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "font catalogue error: {self:?}")
    }
}
impl std::error::Error for FontError {}

/// CoreText-style normalized metadata; not CSS numeric weight or a matching rule.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FontTraits {
    pub weight: f64,
    pub width: f64,
    pub slant: f64,
    pub bold: bool,
    pub italic: bool,
    pub monospace: bool,
    pub condensed: bool,
    pub expanded: bool,
}
impl FontTraits {
    fn valid(self) -> bool {
        [self.weight, self.width, self.slant]
            .iter()
            .all(|value| value.is_finite() && (-1.0..=1.0).contains(value))
    }
}
/// Discovery metadata can describe a resource that this loader refuses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FontAvailability {
    /// Resource admitted by the snapshot policy, not global descriptor registration.
    pub enabled: bool,
    pub downloadable: bool,
    pub downloaded: bool,
}
/// Immutable names/traits. Family/PostScript are required; optional names may be empty.
#[derive(Clone, Debug, PartialEq)]
pub struct FontDescriptor {
    family: String,
    style: String,
    display: String,
    postscript: String,
    traits: Option<FontTraits>,
    diagnostic: Option<FontError>,
    availability: FontAvailability,
}
impl FontDescriptor {
    pub fn new(
        family: &str,
        style: &str,
        display: &str,
        postscript: &str,
        traits: FontTraits,
        availability: FontAvailability,
    ) -> Result<Self, FontError> {
        if family.is_empty() || postscript.is_empty() || !traits.valid() {
            return Err(FontError::InvalidInput);
        }
        for name in [family, style, display, postscript] {
            if name.len() > MAXIMUM_FONT_NAME_BYTES {
                return Err(FontError::Capacity);
            }
            if name.chars().any(char::is_control) {
                return Err(FontError::InvalidInput);
            }
        }
        Ok(Self {
            family: family.into(),
            style: style.into(),
            display: display.into(),
            postscript: postscript.into(),
            traits: Some(traits),
            diagnostic: (!availability.enabled || availability.downloadable)
                .then_some(FontError::Unavailable),
            availability,
        })
    }
    pub fn family(&self) -> &str {
        &self.family
    }
    pub fn style(&self) -> &str {
        &self.style
    }
    pub fn display_name(&self) -> &str {
        &self.display
    }
    pub fn postscript_name(&self) -> &str {
        &self.postscript
    }
    /// None means the native metadata could not be admitted as normalized traits.
    pub fn traits(&self) -> Option<FontTraits> {
        self.traits
    }
    /// Explicit discovered-resource refusal, without substituting another style.
    pub fn diagnostic(&self) -> Option<FontError> {
        self.diagnostic
    }
    #[cfg(target_os = "macos")]
    fn native_unsupported(mut self, invalid_traits: bool, diagnostic: FontError) -> Self {
        if invalid_traits {
            self.traits = None;
        }
        self.diagnostic = Some(diagnostic);
        self
    }
    pub fn availability(&self) -> FontAvailability {
        self.availability
    }
    fn metadata_bytes(&self) -> usize {
        self.family.len() + self.style.len() + self.display.len() + self.postscript.len()
    }
}
/// Opaque same-snapshot selection, not a persistent system identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontSelection {
    owner: u64,
    index: usize,
}
/// A sorted copied snapshot. Duplicate labels remain individually selectable.
pub struct FontCatalog {
    owner: u64,
    entries: Vec<FontDescriptor>,
    #[cfg(any(target_os = "macos", test))]
    source_indices: Vec<usize>,
}
static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);
impl FontCatalog {
    pub fn new(descriptors: Vec<FontDescriptor>) -> Result<Self, FontError> {
        Self::new_with_owner(descriptors, &NEXT_OWNER)
    }
    fn new_with_owner(
        descriptors: Vec<FontDescriptor>,
        next: &AtomicU64,
    ) -> Result<Self, FontError> {
        if descriptors.len() > MAXIMUM_FONT_ENTRIES {
            return Err(FontError::Capacity);
        }
        let mut bytes = 0usize;
        for descriptor in &descriptors {
            bytes = bytes
                .checked_add(descriptor.metadata_bytes())
                .ok_or(FontError::Capacity)?;
            if bytes > MAXIMUM_FONT_METADATA_BYTES {
                return Err(FontError::Capacity);
            }
        }
        let mut owner = next.load(Ordering::Relaxed);
        loop {
            let following = owner
                .checked_add(1)
                .filter(|_| owner != 0)
                .ok_or(FontError::Capacity)?;
            match next.compare_exchange_weak(owner, following, Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => break,
                Err(actual) => owner = actual,
            }
        }
        let mut indexed: Vec<_> = descriptors.into_iter().enumerate().collect();
        indexed.sort_by(|a, b| {
            (&a.1.family, &a.1.style, &a.1.postscript, a.0).cmp(&(
                &b.1.family,
                &b.1.style,
                &b.1.postscript,
                b.0,
            ))
        });
        #[cfg(any(target_os = "macos", test))]
        let (source_indices, entries) = indexed.into_iter().unzip();
        #[cfg(not(any(target_os = "macos", test)))]
        let entries = indexed
            .into_iter()
            .map(|(_, descriptor)| descriptor)
            .collect();
        Ok(Self {
            owner,
            entries,
            #[cfg(any(target_os = "macos", test))]
            source_indices,
        })
    }
    pub fn entries(&self) -> &[FontDescriptor] {
        &self.entries
    }
    /// Exact case-sensitive unique family names, in catalogue order.
    pub fn families(&self) -> Vec<&str> {
        let mut result = Vec::new();
        for entry in &self.entries {
            if result.last().copied() != Some(entry.family()) {
                result.push(entry.family());
            }
        }
        result
    }
    /// Individual exact-family style selections; equal labels are not merged.
    pub fn styles(&self, family: &str) -> Vec<FontSelection> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.family() == family)
            .map(|(index, _)| FontSelection {
                owner: self.owner,
                index,
            })
            .collect()
    }
    pub fn selection(&self, index: usize) -> Result<FontSelection, FontError> {
        if index >= self.entries.len() {
            return Err(FontError::InvalidInput);
        }
        Ok(FontSelection {
            owner: self.owner,
            index,
        })
    }
    pub fn descriptor(&self, selection: FontSelection) -> Result<&FontDescriptor, FontError> {
        if selection.owner != self.owner {
            return Err(FontError::ForeignSelection);
        }
        self.entries
            .get(selection.index)
            .ok_or(FontError::InvalidInput)
    }
    #[cfg(any(target_os = "macos", test))]
    fn source_index(&self, selection: FontSelection) -> Result<usize, FontError> {
        self.descriptor(selection)?;
        Ok(self.source_indices[selection.index])
    }
}
/// Effective user-space setting. Values are explicitly f32, not arbitrary precision.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontVariation {
    pub tag: [u8; 4],
    pub value: f32,
}
/// Exact source bounds alongside public native metadata. Native bound-only
/// rounding <=1e-4 is tolerated; selected coordinates are never clamped.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SystemFontAxis {
    pub tag: [u8; 4],
    pub minimum: f64,
    pub default: f64,
    pub maximum: f64,
    pub native_minimum: f64,
    pub native_default: f64,
    pub native_maximum: f64,
    pub native_value: f64,
    pub hidden: bool,
}
/// Owned rebuilt face/metadata. No CFData/catalogue borrow survives loading.
#[derive(Clone, Debug)]
pub struct LoadedSystemFont {
    descriptor: FontDescriptor,
    selection: FontSelection,
    bytes: Vec<u8>,
    variations: Vec<FontVariation>,
    axes: Vec<SystemFontAxis>,
}
impl LoadedSystemFont {
    pub fn descriptor(&self) -> &FontDescriptor {
        &self.descriptor
    }
    pub fn selection(&self) -> FontSelection {
        self.selection
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn axes(&self) -> &[SystemFontAxis] {
        &self.axes
    }
    pub fn variations(&self) -> &[FontVariation] {
        &self.variations
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn descriptor(family: &str, style: &str, postscript: &str) -> FontDescriptor {
        FontDescriptor::new(
            family,
            style,
            "",
            postscript,
            FontTraits::default(),
            FontAvailability {
                enabled: true,
                ..Default::default()
            },
        )
        .unwrap()
    }
    #[test]
    fn exact_styles_sorting_source_identity_and_foreign_owners() {
        let catalog = FontCatalog::new(vec![
            descriptor("Zulu", "Regular", "Z"),
            descriptor("Alpha", "Bold", "AB2"),
            descriptor("Alpha", "Bold", "AB1"),
            descriptor("alpha", "", "a"),
        ])
        .unwrap();
        assert_eq!(catalog.families(), ["Alpha", "Zulu", "alpha"]);
        let styles = catalog.styles("Alpha");
        assert_eq!(styles.len(), 2);
        assert_eq!(
            catalog.descriptor(styles[0]).unwrap().postscript_name(),
            "AB1"
        );
        assert_eq!(catalog.source_index(styles[0]), Ok(2));
        assert!(catalog.styles("ALPHA").is_empty());
        let other = FontCatalog::new(vec![descriptor("Alpha", "Bold", "AB1")]).unwrap();
        assert_eq!(
            other.descriptor(styles[0]),
            Err(FontError::ForeignSelection)
        );
        assert_eq!(catalog.selection(4), Err(FontError::InvalidInput));
    }
    #[test]
    fn names_traits_metadata_and_owner_exhaustion_are_complete() {
        for name in ["", "a\0b", "a\nb", "a\u{85}b"] {
            assert!(
                FontDescriptor::new(name, "", "", "p", FontTraits::default(), Default::default())
                    .is_err()
            );
        }
        assert_eq!(
            FontDescriptor::new(
                &"a".repeat(1025),
                "",
                "",
                "p",
                FontTraits::default(),
                Default::default()
            ),
            Err(FontError::Capacity)
        );
        for value in [f64::NAN, f64::INFINITY, 1.01, -1.01] {
            assert_eq!(
                FontDescriptor::new(
                    "a",
                    "",
                    "",
                    "p",
                    FontTraits {
                        weight: value,
                        ..Default::default()
                    },
                    Default::default()
                ),
                Err(FontError::InvalidInput)
            );
        }
        let d = descriptor("a", "", "p");
        assert!(matches!(
            FontCatalog::new(vec![d; 4097]),
            Err(FontError::Capacity)
        ));
        let d = FontDescriptor::new(
            &"f".repeat(1024),
            &"s".repeat(1024),
            &"d".repeat(1024),
            &"p".repeat(1024),
            FontTraits::default(),
            Default::default(),
        )
        .unwrap();
        assert!(FontCatalog::new(vec![d.clone(); 1024]).is_ok());
        assert!(matches!(
            FontCatalog::new(vec![d; 1025]),
            Err(FontError::Capacity)
        ));
        let exhausted = AtomicU64::new(u64::MAX);
        assert!(matches!(
            FontCatalog::new_with_owner(vec![], &exhausted),
            Err(FontError::Capacity)
        ));
        assert_eq!(exhausted.load(Ordering::Relaxed), u64::MAX);
        assert!(FontCatalog::new(vec![]).unwrap().entries().is_empty());
    }
}
