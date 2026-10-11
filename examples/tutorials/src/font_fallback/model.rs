use ravet_surface::{
    drawing::{Color, SvgDrawing},
    fallback::TextFontStack,
    raster::{RasterCanvas, RasterText},
    text::{TextDirection, TextError, TextFont, TextOptions},
};
use std::fmt::Write;
pub const SAMPLES: [(&str, &str, [u8; 4], bool); 5] = [
    ("latin", "abcffi", *b"Latn", false),
    ("marks", "á a\u{301}", *b"Latn", false),
    ("arabic", "تبت", *b"Arab", true),
    ("arabicmarks", "تُبَت", *b"Arab", true),
    ("missing", "👩\u{200d}💻 ☃", *b"Latn", false),
];
pub const SIZES: [f64; 3] = [14., 20., 28.];
pub struct Model {
    fonts: [TextFont; 3],
    pub sample: usize,
    pub order: usize,
    pub size: usize,
    pub raster: bool,
    pub revision: u32,
    pub snapshot: String,
}
impl Model {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let fonts = [
            TextFont::from_bytes(
                include_bytes!(
                    "../../../../crates/ravet-surface/tests/fixtures/fallback/RavetFallbackPrimary.ttf"
                ),
                0,
            )?,
            TextFont::from_bytes(
                include_bytes!(
                    "../../../../crates/ravet-surface/tests/fixtures/fallback/RavetFallbackComplement.ttf"
                ),
                0,
            )?,
            TextFont::from_bytes(
                include_bytes!(
                    "../../../../crates/ravet-surface/tests/fixtures/text/NotoSansArabic.ttf"
                ),
                0,
            )?,
        ];
        let snapshot = prepare(&fonts, 0, 0, 0, false, 0)?;
        Ok(Self {
            fonts,
            sample: 0,
            order: 0,
            size: 0,
            raster: false,
            revision: 0,
            snapshot,
        })
    }
    pub fn message(&mut self, json: &str) -> bool {
        if json.len() > 64 {
            return false;
        }
        let selection = json
            .strip_prefix("{\"sample\":\"")
            .and_then(|s| s.strip_suffix("\"}"))
            .and_then(|s| SAMPLES.iter().position(|p| p.0 == s));
        let (sample, order, size, raster) = if let Some(sample) = selection {
            (sample, self.order, self.size, self.raster)
        } else {
            match json {
                "{\"order\":\"explicit\"}" => (self.sample, 0, self.size, self.raster),
                "{\"order\":\"noto\"}" => (self.sample, 1, self.size, self.raster),
                "{\"size\":\"14\"}" => (self.sample, self.order, 0, self.raster),
                "{\"size\":\"20\"}" => (self.sample, self.order, 1, self.raster),
                "{\"size\":\"28\"}" => (self.sample, self.order, 2, self.raster),
                "{\"mode\":\"svg\"}" => (self.sample, self.order, self.size, false),
                "{\"mode\":\"raster\"}" => (self.sample, self.order, self.size, true),
                _ => return false,
            }
        };
        if (sample, order, size, raster) == (self.sample, self.order, self.size, self.raster) {
            return true;
        }
        let Some(revision) = self.revision.checked_add(1) else {
            return false;
        };
        let Ok(snapshot) = prepare(&self.fonts, sample, order, size, raster, revision) else {
            return false;
        };
        self.sample = sample;
        self.order = order;
        self.size = size;
        self.raster = raster;
        self.revision = revision;
        self.snapshot = snapshot;
        true
    }
}
pub fn prepare(
    fonts: &[TextFont; 3],
    sample: usize,
    order: usize,
    size: usize,
    raster: bool,
    revision: u32,
) -> Result<String, Box<dyn std::error::Error>> {
    let sample = *SAMPLES.get(sample).ok_or(TextError::InvalidInput)?;
    let size = *SIZES.get(size).ok_or(TextError::InvalidInput)?;
    let indexes = match order {
        0 => [0, 1, 2],
        1 => [2, 1, 0],
        _ => return Err(TextError::InvalidInput.into()),
    };
    let selected = indexes.map(|index| &fonts[index]);
    let prepared = TextFontStack::new(&selected)?.shape(
        sample.1,
        TextOptions {
            size,
            direction: if sample.3 {
                TextDirection::RightToLeft
            } else {
                TextDirection::LeftToRight
            },
            script: Some(sample.2),
            language: Some(if sample.3 { "ar" } else { "en" }),
            features: &[],
        },
    )?;
    let payload = if raster {
        let text = RasterText::from_fallback(&prepared, (8., 45.))?;
        let mut canvas = RasterCanvas::new(128, 64, 1)?;
        canvas.clear(Color::rgb(255, 255, 255));
        canvas.draw_text(&text, Color::rgb(32, 62, 96), true)?;
        base64(canvas.pixels())
    } else {
        let mut svg =
            SvgDrawing::new(128., 64., sample.1).map_err(|e| format!("SVG viewport: {e:?}"))?;
        svg.fallback_text(&prepared, (8., 45.), Color::rgb(32, 62, 96))?;
        base64(svg.finish().as_bytes())
    };
    let source = base64(sample.1.as_bytes());
    let mut snapshot = format!(
        "{{\"revision\":{revision},\"sample\":\"{}\",\"order\":\"{}\",\"size\":{size},\"mode\":\"{}\",\"source\":\"{source}\",\"payload\":\"{payload}\",\"missing\":{},\"advance\":{},\"runs\":[",
        sample.0,
        if order == 0 { "explicit" } else { "noto" },
        if raster { "raster" } else { "svg" },
        prepared.missing_glyphs(),
        prepared.advance().0
    );
    for (i, run) in prepared.runs().iter().enumerate() {
        if i > 0 {
            snapshot.push(',');
        }
        write!(
            snapshot,
            "{{\"face\":{},\"start\":{},\"end\":{},\"x\":{},\"glyphs\":[",
            indexes[run.font_index()],
            run.range().start,
            run.range().end,
            run.origin().0
        )?;
        for (j, glyph) in run.shape().glyphs().iter().enumerate() {
            if j > 0 {
                snapshot.push(',');
            }
            write!(
                snapshot,
                "[{},{}]",
                glyph.id,
                run.source_cluster(j).unwrap()
            )?;
        }
        snapshot.push_str("]}");
    }
    snapshot.push_str("],\"io\":{\"audio\":false,\"input\":false,\"midi\":false}}");
    if snapshot.len() > 65536 {
        return Err(TextError::Capacity.into());
    }
    Ok(snapshot)
}
pub fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for bytes in bytes.chunks(3) {
        let a = bytes[0];
        let b = bytes.get(1).copied().unwrap_or(0);
        let c = bytes.get(2).copied().unwrap_or(0);
        for ch in [
            TABLE[(a >> 2) as usize],
            TABLE[(((a & 3) << 4) | (b >> 4)) as usize],
            if bytes.len() > 1 {
                TABLE[(((b & 15) << 2) | (c >> 6)) as usize]
            } else {
                b'='
            },
            if bytes.len() > 2 {
                TABLE[(c & 63) as usize]
            } else {
                b'='
            },
        ]
        .into_iter()
        {
            out.push(ch as char);
        }
    }
    out
}
// The native fixture prepares independent expected snapshots.
#[cfg(all(target_os = "macos", feature = "live-ui"))]
pub fn prepare_for(
    model: &Model,
    sample: usize,
    order: usize,
    size: usize,
    raster: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    prepare(&model.fonts, sample, order, size, raster, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_prepared_selection_stays_bounded_and_models_publish_whole() {
        let mut model = Model::new().unwrap();
        for sample in 0..SAMPLES.len() {
            for order in 0..2 {
                for size in 0..3 {
                    for raster in [false, true] {
                        let snapshot =
                            prepare(&model.fonts, sample, order, size, raster, 0).unwrap();
                        assert!(snapshot.len() < 65536);
                    }
                }
            }
        }
        let original = model.snapshot.clone();
        assert!(!model.message("{\"size\":\"NaN\"}"));
        assert_eq!(model.snapshot, original);
        assert!(model.message("{\"mode\":\"raster\"}"));
        assert_eq!(model.revision, 1);
        assert!(model.snapshot.contains("\"mode\":\"raster\""));
        model.revision = u32::MAX;
        let frozen = model.snapshot.clone();
        assert!(!model.message("{\"sample\":\"marks\"}"));
        assert_eq!(model.sample, 0);
        assert_eq!(model.snapshot, frozen);
        assert!(model.message("{\"mode\":\"raster\"}"));
    }
}
