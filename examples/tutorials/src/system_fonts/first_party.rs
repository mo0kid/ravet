use super::model::*;
use ravet_surface::text::*;
use ravet_webview::fonts::*;
struct Source {
    catalog: FontCatalog,
}
impl FontSource for Source {
    fn catalog(&self) -> &FontCatalog {
        &self.catalog
    }
    fn load(&self, s: FontSelection) -> Result<TextFont, FontError> {
        let d = self.catalog.descriptor(s)?;
        let base = TextFont::from_bytes(
            include_bytes!(
                "../../../../crates/ravet-surface/tests/fixtures/variation/RavetVariation.ttf"
            ),
            0,
        )
        .map_err(|_| FontError::UnsupportedFont)?;
        let settings = match d.style() {
            "Heavy" => vec![TextVariation {
                tag: *b"wght",
                value: 900.,
            }],
            "Wide" => vec![TextVariation {
                tag: *b"wdth",
                value: 200.,
            }],
            _ => vec![],
        };
        base.with_variations(&settings)
            .map_err(|_| FontError::UnsupportedFont)
    }
}
pub fn run() {
    let d = ["Regular", "Heavy", "Wide"]
        .into_iter()
        .map(|style| {
            FontDescriptor::new(
                "RAVET first-party rectangles",
                style,
                "",
                &format!("RavetVariation-{style}"),
                FontTraits::default(),
                FontAvailability {
                    enabled: true,
                    ..Default::default()
                },
            )
            .unwrap()
        })
        .collect();
    let source = Source {
        catalog: FontCatalog::new(d).unwrap(),
    };
    let initial = source.catalog.selection(0).unwrap();
    let mut m = Model::new(source, initial).unwrap();
    let mut snapshots = Vec::new();
    for selected in 0..3 {
        m.select(m.catalog().selection(selected).unwrap()).unwrap();
        for sample in 0..2 {
            for size in 0..3 {
                for mode in 0..2 {
                    for (key, value) in [("sample", sample), ("size", size), ("mode", mode)] {
                        assert!(m.message(&format!(
                            "{{\"{key}\":{value},\"revision\":{}}}",
                            m.revision()
                        )))
                    }
                    assert_eq!(m.selected(), m.catalog().selection(selected).unwrap());
                    assert!(m.payload().len() <= 65536 && m.diagnostic().is_none());
                    snapshots.push(m.snapshot.clone());
                }
            }
        }
    }
    println!("[{}]", snapshots.join(","));
}
