//! First-party analytic source used solely by acceptance/browser fixtures.
use crate::model::FontSource;
use ravet_surface::text::*;
use ravet_webview::fonts::*;
pub struct Source {
    catalog: FontCatalog,
}
impl Source {
    pub fn new() -> Self {
        let descriptors = [
            "CFF1", "CFF2 100", "CFF2 400", "CFF2 650", "CFF2 900", "Capacity", "Invalid",
        ]
        .into_iter()
        .map(|style| {
            FontDescriptor::new(
                "RAVET authored PostScript",
                style,
                "",
                &format!("RavetAuthored-{style}"),
                FontTraits::default(),
                FontAvailability {
                    enabled: true,
                    ..Default::default()
                },
            )
            .unwrap()
        })
        .collect();
        Self {
            catalog: FontCatalog::new(descriptors).unwrap(),
        }
    }
}
impl Default for Source {
    fn default() -> Self {
        Self::new()
    }
}
impl FontSource for Source {
    fn catalog(&self) -> &FontCatalog {
        &self.catalog
    }
    fn load(&self, s: FontSelection) -> Result<TextFont, FontError> {
        let style = self.catalog.descriptor(s)?.style();
        let bytes: &[u8] = match style {
            "Capacity" => include_bytes!(
                "../../../../crates/ravet-surface/tests/fixtures/postscript/consumer-capacity-cff.otf"
            ),
            "Invalid" => include_bytes!(
                "../../../../crates/ravet-surface/tests/fixtures/postscript/invalid-private-bluevalues-cff.otf"
            ),
            "CFF1" => {
                include_bytes!(
                    "../../../../crates/ravet-surface/tests/fixtures/postscript/consumer-cff.otf"
                )
            }
            _ => include_bytes!(
                "../../../../crates/ravet-surface/tests/fixtures/postscript/consumer-cff2.otf"
            ),
        };
        let base = TextFont::from_bytes(bytes, 0).map_err(|_| FontError::UnsupportedFont)?;
        base.outline_format()
            .map_err(|_| FontError::UnsupportedFont)?;
        if let Some(user) = style.strip_prefix("CFF2 ") {
            base.with_variations(&[TextVariation {
                tag: *b"wght",
                value: user.parse().map_err(|_| FontError::InvalidInput)?,
            }])
            .map_err(|_| FontError::UnsupportedFont)
        } else {
            Ok(base)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Model;
    #[test]
    fn actual_cff_styles_and_coordinates_render_through_both_model_paths() {
        let source = Source::new();
        let initial = source.catalog.selection(0).unwrap();
        let mut m = Model::new(source, initial).unwrap();
        for style in ["CFF1", "CFF2 100", "CFF2 400", "CFF2 650", "CFF2 900"] {
            let selection = m
                .catalog()
                .styles("RAVET authored PostScript")
                .into_iter()
                .find(|s| m.catalog().descriptor(*s).unwrap().style() == style)
                .unwrap();
            m.select(selection).unwrap();
            assert_eq!(m.selected(), selection);
            for mode in [0, 1] {
                assert!(m.message(&format!(
                    "{{\"mode\":{mode},\"revision\":{}}}",
                    m.revision()
                )));
                assert!(m.payload().len() < 65000);
                assert_eq!(m.diagnostic(), None);
            }
        }
    }
    #[test]
    fn native_publication_model_keeps_prior_cff_image_after_actual_late_outline_failures() {
        let source = Source::new();
        let initial = source.catalog.selection(0).unwrap();
        let mut m = Model::new(source, initial).unwrap();
        for mode in [0, 1] {
            assert!(m.message(&format!(
                "{{\"mode\":{mode},\"revision\":{}}}",
                m.revision()
            )));
            for (style, error) in [
                ("Capacity", FontError::Capacity),
                ("Invalid", FontError::UnsupportedFont),
            ] {
                let before = m.payload().to_owned();
                let selected = m.selected();
                let revision = m.revision();
                let selection = m
                    .catalog()
                    .styles("RAVET authored PostScript")
                    .into_iter()
                    .find(|s| m.catalog().descriptor(*s).unwrap().style() == style)
                    .unwrap();
                assert_eq!(m.select(selection), Err(error));
                assert_eq!(m.selected(), selected);
                assert_eq!(m.payload(), before);
                assert_eq!(m.revision(), revision + 1);
                assert_eq!(m.diagnostic(), Some(error));
            }
        }
    }
}
