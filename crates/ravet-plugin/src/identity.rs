//! Format identities derived deterministically from the plug-in ID.
//!
//! The derivations match the identities RAVET Foundry has generated since
//! 0.0.1, and never change within a major version. Products that must keep
//! identities from an earlier framework override them with [`IdentityOverrides`].
use crate::product::{Category, Product, ProductError};
use ravet_foundation::AudioLayout;
use sha2::{Digest, Sha256};

/// Identities a product keeps from elsewhere. Unset fields are derived.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IdentityOverrides<'a> {
    pub clap_id: Option<&'a str>,
    /// VST3 processor and controller class IDs, as four 32-bit words each.
    pub vst3_classes: Option<([u32; 4], [u32; 4])>,
    pub au_subtype: Option<[u8; 4]>,
    pub au_manufacturer: Option<[u8; 4]>,
    pub aax_manufacturer: Option<u32>,
    pub aax_product: Option<u32>,
    pub lv2_uri: Option<&'a str>,
}
impl IdentityOverrides<'_> {
    /// No overrides; every identity is derived.
    pub const NONE: Self = Self {
        clap_id: None,
        vst3_classes: None,
        au_subtype: None,
        au_manufacturer: None,
        aax_manufacturer: None,
        aax_product: None,
        lv2_uri: None,
    };
}

/// Resolved identities for every format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identities {
    pub clap_id: String,
    pub vst3_processor: [u32; 4],
    pub vst3_controller: [u32; 4],
    /// `aufx`, `aumu` or `aumf`, from the product category.
    pub au_type: [u8; 4],
    pub au_subtype: [u8; 4],
    pub au_manufacturer: [u8; 4],
    pub aax_manufacturer: u32,
    pub aax_product: u32,
    /// One AAX plug-in type ID per supported layout, in layout order.
    pub aax_types: Vec<(AudioLayout, u32)>,
    pub lv2_uri: String,
}

fn digest(text: &str) -> [u8; 32] {
    Sha256::digest(text.as_bytes()).into()
}
fn class_id(plugin_id: &str, role: &str) -> [u32; 4] {
    let hash = digest(&format!("{plugin_id}/{role}"));
    core::array::from_fn(|i| u32::from_be_bytes(hash[i * 4..i * 4 + 4].try_into().unwrap()))
}
fn four_letters(text: &str) -> [u8; 4] {
    let hash = digest(text);
    core::array::from_fn(|i| b'A' + hash[i] % 26)
}
fn word(text: &str) -> u32 {
    u32::from_be_bytes(digest(text)[..4].try_into().unwrap())
}
fn layout_key(layout: AudioLayout) -> String {
    match (layout.input_channels(), layout.output_channels()) {
        (1, 1) => "MONO".into(),
        (2, 2) => "STEREO".into(),
        (inputs, outputs) => format!("{inputs}x{outputs}"),
    }
}
fn valid_code(code: [u8; 4]) -> bool {
    code.iter().all(|c| c.is_ascii_graphic() || *c == b' ') && code[0] != b' '
}

impl Identities {
    pub(crate) fn resolve(product: &Product<'_>) -> Result<Self, ProductError> {
        let id = product.plugin_id();
        let overrides = product.identity_overrides();
        let clap_id = overrides.clap_id.unwrap_or(id);
        if clap_id.is_empty() || clap_id.len() > 255 || clap_id.chars().any(char::is_control) {
            return Err(ProductError::InvalidIdentity("clap_id"));
        }
        let lv2_uri = overrides
            .lv2_uri
            .map(str::to_owned)
            .unwrap_or_else(|| format!("urn:{id}"));
        if !lv2_uri.contains(':')
            || lv2_uri.len() > 1024
            || lv2_uri
                .chars()
                .any(|c| c.is_control() || c.is_whitespace() || "<>\"{}|\\^`".contains(c))
        {
            return Err(ProductError::InvalidIdentity("lv2_uri"));
        }
        let (vst3_processor, vst3_controller) = overrides
            .vst3_classes
            .unwrap_or_else(|| (class_id(id, "processor"), class_id(id, "controller")));
        if vst3_processor == vst3_controller {
            return Err(ProductError::InvalidIdentity("vst3_classes"));
        }
        let au_subtype = overrides.au_subtype.unwrap_or_else(|| four_letters(id));
        let au_manufacturer = overrides
            .au_manufacturer
            .unwrap_or_else(|| four_letters(product.vendor()));
        for (field, code) in [
            ("au_subtype", au_subtype),
            ("au_manufacturer", au_manufacturer),
        ] {
            if !valid_code(code) {
                return Err(ProductError::InvalidIdentity(field));
            }
        }
        let aax_types: Vec<_> = product
            .layouts()
            .iter()
            .map(|layout| (*layout, word(&format!("{id}/{}", layout_key(*layout)))))
            .collect();
        for (index, (_, type_id)) in aax_types.iter().enumerate() {
            if aax_types[..index].iter().any(|(_, other)| other == type_id) {
                return Err(ProductError::InvalidIdentity("aax_types"));
            }
        }
        Ok(Self {
            clap_id: clap_id.into(),
            vst3_processor,
            vst3_controller,
            au_type: *match product.category() {
                Category::Effect => b"aufx",
                Category::Instrument => b"aumu",
                Category::MidiEffect => b"aumf",
            },
            au_subtype,
            au_manufacturer,
            aax_manufacturer: overrides
                .aax_manufacturer
                .unwrap_or_else(|| word(&format!("{id}/MANUFACTURER"))),
            aax_product: overrides
                .aax_product
                .unwrap_or_else(|| word(&format!("{id}/PRODUCT"))),
            aax_types,
            lv2_uri,
        })
    }
}
