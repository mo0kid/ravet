use ravet_surface::{
    color::{AlphaMode, ColorEncoding, RgbaView},
    svg::{SvgDocument, SvgFit},
};
use std::fmt::Write as _;
pub const ASSETS: [(&str, &str); 3] = [
    (
        "scene",
        include_str!("../../../../crates/ravet-surface/tests/fixtures/svg/scene.svg"),
    ),
    ("logo", include_str!("../../../../media/ravet_LOGO.svg")),
    (
        "integer",
        include_str!("../../../../crates/ravet-surface/tests/fixtures/svg/integer.svg"),
    ),
];
pub const SIZES: [(u32, u32); 3] = [(128, 64), (96, 48), (160, 48)];
#[cfg(any(all(target_os = "macos", feature = "live-ui"), test))]
pub struct Model {
    docs: [SvgDocument; 3],
    pub source: usize,
    pub fit: SvgFit,
    pub size: usize,
    pub revision: u32,
    pub snapshot: String,
}
#[cfg(any(all(target_os = "macos", feature = "live-ui"), test))]
impl Default for Model {
    fn default() -> Self {
        Self::new()
    }
}
#[cfg(any(all(target_os = "macos", feature = "live-ui"), test))]
impl Model {
    pub fn new() -> Self {
        let docs = ASSETS.map(|(_, s)| SvgDocument::parse(s.as_bytes()).unwrap());
        let snapshot = prepare(&docs[0], 0, SvgFit::Contain, 0, 0).unwrap();
        Self {
            docs,
            source: 0,
            fit: SvgFit::Contain,
            size: 0,
            revision: 0,
            snapshot,
        }
    }
    pub fn message(&mut self, json: &str) -> bool {
        if json.len() > 64 {
            return false;
        }
        let (source, fit, size) = match json {
            "{\"source\":\"scene\"}" => (0, self.fit, self.size),
            "{\"source\":\"logo\"}" => (1, self.fit, self.size),
            "{\"source\":\"integer\"}" => (2, self.fit, self.size),
            "{\"fit\":\"contain\"}" => (self.source, SvgFit::Contain, self.size),
            "{\"fit\":\"stretch\"}" => (self.source, SvgFit::Stretch, self.size),
            "{\"size\":\"128x64\"}" => (self.source, self.fit, 0),
            "{\"size\":\"96x48\"}" => (self.source, self.fit, 1),
            "{\"size\":\"160x48\"}" => (self.source, self.fit, 2),
            _ => return false,
        };
        if (source, fit, size) == (self.source, self.fit, self.size) {
            return true;
        }
        let Some(revision) = self.revision.checked_add(1) else {
            return false;
        };
        let Ok(snapshot) = prepare(&self.docs[source], source, fit, size, revision) else {
            return false;
        };
        self.source = source;
        self.fit = fit;
        self.size = size;
        self.revision = revision;
        self.snapshot = snapshot;
        true
    }
}
pub fn prepare(
    doc: &SvgDocument,
    source: usize,
    fit: SvgFit,
    size: usize,
    revision: u32,
) -> Result<String, Box<dyn std::error::Error>> {
    let (w, h) = SIZES[size];
    let im = doc.render(w, h, fit)?;
    let view = RgbaView::new(
        w,
        h,
        im.pixels(),
        ColorEncoding::Srgb,
        AlphaMode::Premultiplied,
    )?;
    let mut straight = vec![0; im.pixels().len()];
    view.convert_into(ColorEncoding::Srgb, AlphaMode::Straight, &mut straight)?;
    let s = format!(
        "{{\"io\":{{\"audio\":false,\"input\":false,\"midi\":false}},\"source\":\"{}\",\"fit\":\"{}\",\"size\":\"{w}x{h}\",\"width\":{w},\"height\":{h},\"revision\":{revision},\"straight\":\"{}\"}}",
        ASSETS[source].0,
        if fit == SvgFit::Contain {
            "contain"
        } else {
            "stretch"
        },
        base64(&straight)
    );
    if s.len() > 65536 {
        return Err("Snapshot capacity".into());
    }
    Ok(s)
}
pub fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for p in bytes.chunks(3) {
        let word = (u32::from(p[0]) << 16)
            | (u32::from(*p.get(1).unwrap_or(&0)) << 8)
            | u32::from(*p.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i > p.len() {
                s.push('=');
            } else {
                s.push(A[((word >> (18 - 6 * i)) & 63) as usize] as char);
            }
        }
    }
    s
}
pub fn all_snapshots() -> String {
    let mut out = String::from("{");
    let mut first = true;
    for (source, (name, svg)) in ASSETS.iter().enumerate() {
        let doc = SvgDocument::parse(svg.as_bytes()).unwrap();
        for fit in [SvgFit::Contain, SvgFit::Stretch] {
            for (size, (w, h)) in SIZES.iter().enumerate() {
                if !first {
                    out.push(',');
                }
                first = false;
                write!(
                    out,
                    "\"{name}:{}:{w}x{h}\":{}",
                    if fit == SvgFit::Contain {
                        "contain"
                    } else {
                        "stretch"
                    },
                    prepare(&doc, source, fit, size, 0).unwrap()
                )
                .unwrap();
            }
        }
    }
    out.push('}');
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_messages_atomic_noops_and_exhaustion() {
        let mut m = Model::new();
        let first = m.snapshot.clone();
        for s in [
            "{\"source\":\"bad\"}",
            "{\"fit\":\"contain\",\"size\":\"96x48\"}",
            "{ \"source\":\"logo\"}",
            "{\"size\":\"4096x4096\"}",
        ] {
            assert!(!m.message(s));
            assert_eq!(m.snapshot, first);
        }
        assert!(m.message("{\"source\":\"scene\"}"));
        assert_eq!(m.revision, 0);
        assert!(m.message("{\"source\":\"logo\"}"));
        assert_eq!(m.revision, 1);
        m.revision = u32::MAX;
        let before = m.snapshot.clone();
        assert!(!m.message("{\"fit\":\"stretch\"}"));
        assert_eq!(m.snapshot, before);
        assert_eq!(m.fit, SvgFit::Contain);
    }
    #[test]
    fn base64_and_all_snapshot_budgets() {
        for (input, expected) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(input.as_bytes()), expected);
        }
        for (i, (_, s)) in ASSETS.iter().enumerate() {
            let d = SvgDocument::parse(s.as_bytes()).unwrap();
            for fit in [SvgFit::Contain, SvgFit::Stretch] {
                for size in 0..3 {
                    assert!(prepare(&d, i, fit, size, u32::MAX).unwrap().len() < 65536);
                }
            }
        }
        assert_eq!(all_snapshots().matches("\"revision\":").count(), 18);
    }
}
