use ravet_surface::color::{AlphaMode, ColorEncoding, RgbaView, composite_source_over};
use std::fmt::Write as _;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Palette {
    White,
    Amber,
}
impl Palette {
    pub fn name(self) -> &'static str {
        match self {
            Self::White => "white",
            Self::Amber => "amber",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "white" => Some(Self::White),
            "amber" => Some(Self::Amber),
            _ => None,
        }
    }
    fn rgb(self) -> [u8; 3] {
        match self {
            Self::White => [255; 3],
            Self::Amber => [255, 180, 60],
        }
    }
}
#[derive(Clone, Debug)]
pub struct Model {
    pub opacity: u8,
    pub palette: Palette,
    pub revision: u32,
    pub snapshot: String,
}
impl Model {
    pub fn new() -> Self {
        Self {
            opacity: 128,
            palette: Palette::White,
            revision: 0,
            snapshot: snapshot(128, Palette::White, 0),
        }
    }
    /// Only compact one-field messages produced by the local page/native serializer.
    /// Unknown/device messages, noncanonical numbers and capacity/exhaustion reject.
    pub fn message(&mut self, json: &str) -> bool {
        if json.len() > 64 {
            return false;
        }
        let (opacity, palette) = if let Some(text) = json
            .strip_prefix("{\"opacity\":")
            .and_then(|s| s.strip_suffix('}'))
        {
            if text.is_empty()
                || text.len() > 3
                || !text.bytes().all(|b| b.is_ascii_digit())
                || (text.len() > 1 && text.starts_with('0'))
            {
                return false;
            }
            let Ok(opacity) = text.parse::<u8>() else {
                return false;
            };
            (opacity, self.palette)
        } else if json == "{\"palette\":\"white\"}" {
            (self.opacity, Palette::White)
        } else if json == "{\"palette\":\"amber\"}" {
            (self.opacity, Palette::Amber)
        } else {
            return false;
        };
        if (opacity, palette) == (self.opacity, self.palette) {
            return true;
        }
        let Some(revision) = self.revision.checked_add(1) else {
            return false;
        };
        let prepared = snapshot(opacity, palette, revision);
        *self = Self {
            opacity,
            palette,
            revision,
            snapshot: prepared,
        };
        true
    }
}
pub fn snapshot(opacity: u8, palette: Palette, revision: u32) -> String {
    const WIDTH: usize = 32;
    const HEIGHT: usize = 8;
    let mut source = [0u8; WIDTH * HEIGHT * 4];
    let mut backdrop = [0u8; WIDTH * HEIGHT * 4];
    let rgb = palette.rgb();
    for (i, (s, b)) in source
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(backdrop.as_chunks_mut::<4>().0)
        .enumerate()
    {
        *s = [rgb[0], rgb[1], rgb[2], opacity];
        let x = i % WIDTH;
        let y = i / WIDTH;
        let grey = if x < WIDTH / 2 {
            0
        } else {
            ((x - WIDTH / 2) * 12 + (y % 2) * 8) as u8
        };
        *b = [grey, grey, grey, 255];
    }
    let foreground = RgbaView::new(
        WIDTH as u32,
        HEIGHT as u32,
        &source,
        ColorEncoding::Srgb,
        AlphaMode::Straight,
    )
    .unwrap();
    let background = RgbaView::new(
        WIDTH as u32,
        HEIGHT as u32,
        &backdrop,
        ColorEncoding::Srgb,
        AlphaMode::Straight,
    )
    .unwrap();
    let mut linear = [0u8; WIDTH * HEIGHT * 4];
    composite_source_over(
        foreground,
        background,
        ColorEncoding::Srgb,
        AlphaMode::Straight,
        &mut linear,
    )
    .unwrap();
    let mut numeric = [0u8; WIDTH * HEIGHT * 4];
    for ((s, b), out) in source
        .as_chunks::<4>()
        .0
        .iter()
        .zip(backdrop.as_chunks::<4>().0)
        .zip(numeric.as_chunks_mut::<4>().0)
    {
        for channel in 0..3 {
            out[channel] = ((u32::from(s[channel]) * u32::from(opacity)
                + u32::from(b[channel]) * (255 - u32::from(opacity))
                + 127)
                / 255) as u8;
        }
        out[3] = 255;
    }
    let mut output = format!(
        "{{\"io\":{{\"audio\":false,\"input\":false,\"midi\":false}},\"width\":{WIDTH},\"height\":{HEIGHT},\"opacity\":{opacity},\"palette\":\"{}\",\"revision\":{revision},\"numeric\":[",
        palette.name()
    );
    for (index, byte) in numeric.iter().enumerate() {
        if index > 0 {
            output.push(',');
        }
        write!(&mut output, "{byte}").unwrap();
    }
    output.push_str("],\"linear\":[");
    for (index, byte) in linear.iter().enumerate() {
        if index > 0 {
            output.push(',');
        }
        write!(&mut output, "{byte}").unwrap();
    }
    output.push_str("]}");
    assert!(output.len() < 16 * 1024);
    output
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_bounded_messages_revision_and_noop() {
        let mut model = Model::new();
        let original = model.snapshot.clone();
        assert!(model.message("{\"opacity\":128}"));
        assert_eq!(model.revision, 0);
        assert_eq!(model.snapshot, original);
        assert!(model.message("{\"opacity\":0}"));
        assert_eq!(model.revision, 1);
        assert_eq!(model.opacity, 0);
        assert!(model.message("{\"palette\":\"amber\"}"));
        assert_eq!(model.revision, 2);
        assert_eq!(model.palette, Palette::Amber);
        for value in [
            "",
            "{\"opacity\":+1}",
            "{\"opacity\":01}",
            "{\"opacity\":256}",
            "{\"opacity\":-1}",
            "{\"opacity\":1.5}",
            "{\"opacity\":1e2}",
            "{\"opacity\":2,\"x\":1}",
            "{\"palette\":\"red\"}",
        ] {
            let prior = model.snapshot.clone();
            let revision = model.revision;
            assert!(!model.message(value));
            assert_eq!(model.snapshot, prior);
            assert_eq!(model.revision, revision);
        }
        assert!(!model.message(&" ".repeat(65)));
        model.revision = u32::MAX;
        let prior = model.snapshot.clone();
        assert!(!model.message("{\"opacity\":255}"));
        assert_eq!(model.opacity, 0);
        assert_eq!(model.snapshot, prior);
        assert_eq!(model.revision, u32::MAX);
    }
    #[test]
    fn consumer_arrays_remain_under_native_snapshot_capacity_for_all_controls() {
        for palette in [Palette::White, Palette::Amber] {
            for alpha in 0..=255 {
                let prepared = snapshot(alpha, palette, u32::MAX);
                assert!(prepared.len() < 16384);
                assert!(prepared.starts_with("{\"io\":{\"audio\":false"));
                assert!(prepared.ends_with("]}"));
            }
        }
    }
}
