use crate::model::*;
use std::cell::Cell;
struct Source {
    catalog: FontCatalog,
    calls: Cell<usize>,
}
impl FontSource for Source {
    fn catalog(&self) -> &FontCatalog {
        &self.catalog
    }
    fn load(&self, s: FontSelection) -> Result<TextFont, FontError> {
        self.calls.set(self.calls.get() + 1);
        let d = self.catalog.descriptor(s)?;
        if d.style() == "Capacity" {
            return Err(FontError::Capacity);
        }
        if d.style() == "Refused" {
            return Err(FontError::UnsupportedFont);
        }
        TextFont::from_bytes(
            include_bytes!(
                "../../../../crates/ravet-surface/tests/fixtures/variation/RavetVariation.ttf"
            ),
            0,
        )
        .map_err(|_| FontError::UnsupportedFont)
    }
}
fn source() -> Source {
    let mut descriptors = Vec::new();
    for family in ["A", "B", "C", "D", "E"] {
        for style in ["Regular", "Bold", "Italic", "Heavy", "Refused", "Capacity"] {
            descriptors.push(
                FontDescriptor::new(
                    family,
                    style,
                    "display",
                    &format!("{family}-{style}"),
                    FontTraits::default(),
                    FontAvailability {
                        enabled: true,
                        ..Default::default()
                    },
                )
                .unwrap(),
            );
        }
    }
    Source {
        catalog: FontCatalog::new(descriptors).unwrap(),
        calls: Cell::new(0),
    }
}
fn model() -> Model<Source> {
    let s = source();
    let selection = s.catalog.styles("A")[0];
    Model::new(s, selection).unwrap()
}
fn msg(m: &mut Model<Source>, key: &str, value: usize) -> bool {
    m.message(&format!(
        "{{\"{key}\":{value},\"revision\":{}}}",
        m.revision()
    ))
}
#[test]
fn exact_selection_failures_preserve_font_image_foreign_and_revision() {
    let mut m = model();
    let selected = m.selected();
    let payload = m.payload().to_owned();
    let snapshot = m.snapshot.clone();
    let foreign = source().catalog.selection(0).unwrap();
    assert_eq!(m.select(foreign), Err(FontError::ForeignSelection));
    assert_eq!(m.snapshot, snapshot);
    let refused = m
        .catalog()
        .styles("A")
        .into_iter()
        .find(|s| m.catalog().descriptor(*s).unwrap().style() == "Refused")
        .unwrap();
    assert_eq!(m.select(refused), Err(FontError::UnsupportedFont));
    assert_eq!(m.selected(), selected);
    assert_eq!(m.payload(), payload);
    assert_eq!(m.diagnostic(), Some(FontError::UnsupportedFont));
    assert_eq!(m.revision(), 1);
    assert!(m.snapshot.contains("UnsupportedFont"));
    assert!(!m.message("{\"mode\":1,\"revision\":0}"));
    assert!(msg(&mut m, "mode", 1));
    assert_eq!(m.diagnostic(), None);
    assert_eq!(m.selected(), selected);
    assert_ne!(m.payload(), payload);
}
#[test]
fn paged_presentation_is_independent_and_all_catalogue_entries_reachable() {
    let mut m = model();
    let selected = m.selected();
    let payload = m.payload().to_owned();
    assert!(msg(&mut m, "familyPage", 1));
    assert!(msg(&mut m, "family", 4));
    assert_eq!(m.selected(), selected);
    assert_eq!(m.payload(), payload);
    assert!(m.snapshot.contains("E-Bold"));
    assert!(!msg(&mut m, "family", 0));
    assert!(!msg(&mut m, "stylePage", 2));
    assert!(msg(&mut m, "stylePage", 1));
    assert!(m.snapshot.contains("E-Regular"));
    assert_eq!(m.source.calls.get(), 1);
    assert!(msg(&mut m, "stylePage", 0));
    let eligible = m.catalog().styles("E")[0];
    let index = (0..m.catalog().entries().len())
        .find(|i| m.catalog().selection(*i) == Ok(eligible))
        .unwrap();
    assert!(msg(&mut m, "font", index));
    assert_eq!(m.selected(), eligible);
    assert!(m.source.calls.get() > 1);
}
#[test]
fn bounded_commands_noops_revision_exhaustion_and_json_escaping() {
    let mut m = model();
    let before = m.snapshot.clone();
    assert!(msg(&mut m, "size", 0));
    assert_eq!(m.snapshot, before);
    for command in [
        "{\"mode\":2,\"revision\":0}",
        "{\"size\":-1,\"revision\":0}",
        "{\"size\":1.5,\"revision\":0}",
        "{\"size\":1,\"revision\":0,\"extra\":1}",
        "{\"font\":99999,\"revision\":0}",
    ] {
        assert!(!m.message(command));
        assert_eq!(m.snapshot, before);
    }
    assert!(m.message("{\"revision\":0,\"size\":0}"));
    assert!(!m.message("{\"revision\":0,\"size\":01}"));
    assert!(!m.message(&"x".repeat(129)));
    assert_eq!(quote("\"\\\n\u{1}"), "\"\\\"\\\\\\n\\u0001\"");
    m.revision = u32::MAX;
    assert!(!msg(&mut m, "mode", 1));
    assert_eq!(m.snapshot, before);
    assert!(!m.raster);
}

#[test]
fn capacity_failure_keeps_admitted_source_image_and_allows_retry() {
    let mut m = model();
    let old = m.selected();
    let image = m.payload().to_owned();
    let capacity = m
        .catalog()
        .styles("A")
        .into_iter()
        .find(|s| m.catalog().descriptor(*s).unwrap().style() == "Capacity")
        .unwrap();
    assert_eq!(m.select(capacity), Err(FontError::Capacity));
    assert_eq!(m.selected(), old);
    assert_eq!(m.payload(), image);
    assert_eq!(m.diagnostic(), Some(FontError::Capacity));
    assert_eq!(m.select(old), Ok(()));
    assert_eq!(m.diagnostic(), None);
    assert_eq!(m.selected(), old);
    assert_eq!(m.payload(), image);
}
