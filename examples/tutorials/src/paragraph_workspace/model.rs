//! Shared bounded model for ordinary/native-fixture/browser paragraph consumers.
use ravet_surface::{
    drawing::{Color, SvgDrawing},
    fallback::TextFontStack,
    fallback_paragraph::*,
    image::{ImageAsset, encode_png},
    paragraph::{ParagraphOptions, TextAlignment},
    raster::{RasterCanvas, RasterText},
    text::{TextDirection, TextError, TextFont},
};
use std::fmt::Write;
pub const SAMPLE_COUNT: usize = 9;
pub const SIZES: [f64; 3] = [14., 20., 28.];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct State {
    pub sample: usize,
    pub width: usize,
    pub size: usize,
    pub rtl: bool,
    pub alignment: usize,
    pub raster: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            sample: 0,
            width: 320,
            size: 1,
            rtl: false,
            alignment: 0,
            raster: false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidInput,
    Capacity,
    Text,
    Raster,
    Image,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "paragraph model: {self:?}")
    }
}
impl std::error::Error for Error {}
impl From<TextError> for Error {
    fn from(e: TextError) -> Self {
        match e {
            TextError::Capacity => Self::Capacity,
            TextError::InvalidInput => Self::InvalidInput,
            _ => Self::Text,
        }
    }
}
fn sample(index: usize) -> Result<(String, Vec<ParagraphScriptSpan>), Error> {
    let (text, ranges) = match index {
        0 => (
            "a ب c abcffi".to_string(),
            vec![(0..2, *b"Latn"), (2..4, *b"Arab"), (4..13, *b"Latn")],
        ),
        1 => ("تبت تُبَت".to_string(), vec![(0..17, *b"Arab")]),
        2 => (
            "a\u{2067}ب ب ب\u{2069}c".to_string(),
            vec![(0..4, *b"Latn"), (4..12, *b"Arab"), (12..16, *b"Latn")],
        ),
        3 => (
            "ab\r\n\nتبت\u{2028}a ب\u{2029}".to_string(),
            vec![
                (0..5, *b"Latn"),
                (5..14, *b"Arab"),
                (14..16, *b"Latn"),
                (16..21, *b"Arab"),
            ],
        ),
        4 => ("a\u{301} á ffi".to_string(), vec![(0..10, *b"Latn")]),
        5 => ("👩\u{200d}💻 ☃".to_string(), vec![(0..15, *b"Latn")]),
        6 => (String::new(), vec![]),
        7 => (
            "abcffiabcffiabcffiabcffiabcffi".to_string(),
            vec![(0..30, *b"Latn")],
        ),
        8 => ("a\n".repeat(256), vec![(0..512, *b"Latn")]),
        _ => return Err(Error::InvalidInput),
    };
    Ok((
        text,
        ranges
            .into_iter()
            .map(|(range, script)| ParagraphScriptSpan { range, script })
            .collect(),
    ))
}
pub fn authored_fonts() -> Result<[TextFont; 3], Error> {
    Ok([
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
    ])
}
pub struct Model {
    fonts: [TextFont; 3],
    source_name: String,
    state: State,
    revision: u32,
    delivery: u32,
    refresh_exhausted: bool,
    prepared: Prepared,
    diagnostic: Option<Error>,
    pub snapshot: String,
}
struct Prepared {
    payload: String,
    source: String,
    geometry: String,
    width: u32,
    height: u32,
    pixel_hash: u32,
    missing: usize,
    lines: usize,
    runs: usize,
}
impl Model {
    pub fn new(fonts: [TextFont; 3], source_name: &str) -> Result<Self, Error> {
        if source_name.is_empty()
            || source_name.len() > 256
            || source_name.chars().any(char::is_control)
        {
            return Err(Error::InvalidInput);
        }
        let state = State::default();
        let prepared = prepare(&fonts, state)?;
        let snapshot = snapshot(source_name, state, 0, 0, false, &prepared, None)?;
        Ok(Self {
            fonts,
            source_name: source_name.into(),
            state,
            revision: 0,
            delivery: 0,
            refresh_exhausted: false,
            prepared,
            diagnostic: None,
            snapshot,
        })
    }
    #[cfg(all(target_os = "macos", feature = "live-ui"))]
    pub fn delivery(&self) -> u32 {
        self.delivery
    }
    #[cfg(all(target_os = "macos", feature = "live-ui"))]
    pub fn geometry(&self) -> &str {
        &self.prepared.geometry
    }
    /// Own-window fixture injection avoids iterating four billion refreshes;
    /// it changes only private delivery bookkeeping, not model/image state.
    #[cfg(all(target_os = "macos", feature = "live-ui"))]
    pub(super) fn fixture_delivery_near_limit(&mut self) {
        self.delivery = u32::MAX - 1;
    }
    pub fn state(&self) -> State {
        self.state
    }
    #[cfg(any(all(target_os = "macos", feature = "live-ui"), test))]
    pub fn revision(&self) -> u32 {
        self.revision
    }
    #[cfg(any(all(target_os = "macos", feature = "live-ui"), test))]
    pub fn payload(&self) -> &str {
        &self.prepared.payload
    }
    #[cfg(any(all(target_os = "macos", feature = "live-ui"), test))]
    pub fn diagnostic(&self) -> Option<Error> {
        self.diagnostic
    }
    #[cfg(any(all(target_os = "macos", feature = "live-ui"), test))]
    pub fn message(&mut self, json: &str) -> bool {
        // Read-only resynchronization is independent of the caller's stale
        // state revision. The delivery counter bypasses unchanged-JSON native
        // caching; it never changes source, image, selection or state revision.
        if json == "{\"refresh\":1}" {
            if self.refresh_exhausted {
                return false;
            }
            let exhausted = self.delivery == u32::MAX;
            let delivery = if exhausted {
                self.delivery
            } else {
                self.delivery + 1
            };
            let Ok(snapshot) = snapshot(
                &self.source_name,
                self.state,
                self.revision,
                delivery,
                exhausted,
                &self.prepared,
                self.diagnostic,
            ) else {
                return false;
            };
            self.delivery = delivery;
            self.refresh_exhausted = exhausted;
            self.snapshot = snapshot;
            return true;
        }
        if self.delivery == u32::MAX {
            return false;
        }
        let Some((key, value, revision)) = command(json) else {
            return false;
        };
        if revision != self.revision {
            return false;
        }
        let mut next = self.state;
        match key {
            "sample" if value < SAMPLE_COUNT => next.sample = value,
            "width" if (80..=640).contains(&value) => next.width = value,
            "size" if value < SIZES.len() => next.size = value,
            "direction" if value < 2 => next.rtl = value == 1,
            "alignment" if value < 3 => next.alignment = value,
            "mode" if value < 2 => next.raster = value == 1,
            _ => return false,
        }
        self.change(next).is_ok()
    }
    pub fn change(&mut self, next: State) -> Result<(), Error> {
        if self.delivery == u32::MAX {
            return Err(Error::Capacity);
        }
        if next == self.state && self.diagnostic.is_none() {
            return Ok(());
        }
        let revision = self.revision.checked_add(1).ok_or(Error::Capacity)?;
        let prepared = prepare(&self.fonts, next);
        self.finish_change(next, revision, prepared)
    }
    // Preparation and envelope capacity share the same atomic error path.
    fn finish_change(
        &mut self,
        next: State,
        revision: u32,
        prepared: Result<Prepared, Error>,
    ) -> Result<(), Error> {
        let publication = prepared.and_then(|prepared| {
            snapshot(
                &self.source_name,
                next,
                revision,
                self.delivery,
                self.refresh_exhausted,
                &prepared,
                None,
            )
            .map(|snapshot| (prepared, snapshot))
        });
        match publication {
            Ok((prepared, snapshot)) => {
                self.state = next;
                self.revision = revision;
                self.prepared = prepared;
                self.diagnostic = None;
                self.snapshot = snapshot;
                Ok(())
            }
            Err(error) => {
                let snapshot = snapshot(
                    &self.source_name,
                    self.state,
                    revision,
                    self.delivery,
                    self.refresh_exhausted,
                    &self.prepared,
                    Some(error),
                )?;
                self.snapshot = snapshot;
                self.revision = revision;
                self.diagnostic = Some(error);
                Err(error)
            }
        }
    }
    /// Consumer margin policy only: finite content width minus40, floor/clamp80..640.
    /// Too-small/invalid sizes reject; unchanged live dimensions need no transition.
    #[cfg(any(all(target_os = "macos", feature = "live-ui"), test))]
    pub fn resize(&mut self, width: f64) -> Result<(), Error> {
        if !width.is_finite() || width < 120. {
            return Err(Error::InvalidInput);
        }
        self.change(State {
            width: (width - 40.).floor().clamp(80., 640.) as usize,
            ..self.state
        })
    }
}
fn prepare(fonts: &[TextFont; 3], state: State) -> Result<Prepared, Error> {
    if state.sample >= SAMPLE_COUNT
        || !(80..=640).contains(&state.width)
        || state.size >= SIZES.len()
        || state.alignment >= 3
    {
        return Err(Error::InvalidInput);
    }
    let (source, spans) = sample(state.sample)?;
    let size = SIZES[state.size];
    let l = FallbackParagraphLayout::new(
        &TextFontStack::new(&[&fonts[0], &fonts[1], &fonts[2]])?,
        &source,
        FallbackParagraphTextOptions {
            size,
            base_direction: if state.rtl {
                TextDirection::RightToLeft
            } else {
                TextDirection::LeftToRight
            },
            language: None,
            features: &[],
            scripts: &spans,
        },
        ParagraphOptions {
            width: state.width as f64,
            line_height: size * 1.35,
            alignment: match state.alignment {
                0 => TextAlignment::Left,
                1 => TextAlignment::Center,
                _ => TextAlignment::Right,
            },
        },
    )?;
    let width = (l
        .lines()
        .iter()
        .map(|l| l.baseline().0 + l.width())
        .fold(state.width as f64, f64::max)
        + 32.)
        .ceil() as u32;
    let height = (l.height() + size + 32.).ceil().max(64.) as u32;
    if width > 704 || height > 384 {
        return Err(Error::Capacity);
    }
    let baseline = (16., 16. + size);
    let color = Color::rgb(32, 62, 96);
    let mut pixel_hash = 0;
    let payload = if state.raster {
        let text = RasterText::from_fallback_paragraph(&l, baseline).map_err(|_| Error::Raster)?;
        let mut canvas = RasterCanvas::new(width, height, 1).map_err(|_| Error::Raster)?;
        canvas.clear(Color::rgb(255, 255, 255));
        canvas
            .draw_text(&text, color, true)
            .map_err(|_| Error::Raster)?;
        // Opaque-white clear makes premultiplied bytes also exact straight bytes.
        if canvas
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[3] != 255)
        {
            return Err(Error::Raster);
        }
        pixel_hash = canvas.pixels().iter().fold(2166136261u32, |hash, byte| {
            (hash ^ u32::from(*byte)).wrapping_mul(16777619)
        });
        let asset = ImageAsset::from_straight_rgba(width, height, canvas.pixels().to_vec())
            .map_err(|_| Error::Image)?;
        base64(&encode_png(&asset, 45000).map_err(|_| Error::Image)?)
    } else {
        // The visual title excludes forbidden hard controls; original source is separately labelled.
        let mut svg = SvgDrawing::new(width as f64, height as f64, "Prepared paragraph")
            .map_err(|_| Error::InvalidInput)?;
        svg.fallback_paragraph(&l, baseline, color)?;
        let svg = svg.finish().replacen(
            "<svg ",
            &format!("<svg width=\"{width}\" height=\"{height}\" "),
            1,
        );
        if svg.len() > 45000 {
            return Err(Error::Capacity);
        }
        base64(svg.as_bytes())
    };
    let mut geometry = String::from("[");
    let mut runs = 0;
    for (i, line) in l.lines().iter().enumerate() {
        if i > 0 {
            geometry.push(',');
        }
        write!(geometry,"{{\"start\":{},\"end\":{},\"separatorEnd\":{},\"width\":{},\"x\":{},\"y\":{},\"overflow\":{},\"runs\":[",line.range().start,line.range().end,line.separator().end,line.width(),line.baseline().0,line.baseline().1,line.overflows()).unwrap();
        for (j, r) in line.runs().iter().enumerate() {
            runs += 1;
            if j > 0 {
                geometry.push(',');
            }
            write!(
                geometry,
                "[{}, {}, {}, {}, {}]",
                r.font_index(),
                r.range().start,
                r.range().end,
                r.x(),
                r.level()
            )
            .unwrap();
        }
        geometry.push_str("]}");
    }
    geometry.push(']');
    Ok(Prepared {
        payload,
        pixel_hash,
        source: base64(source.as_bytes()),
        geometry,
        width,
        height,
        missing: l.missing_glyphs(),
        lines: l.lines().len(),
        runs,
    })
}
fn snapshot(
    name: &str,
    state: State,
    revision: u32,
    delivery: u32,
    refresh_exhausted: bool,
    p: &Prepared,
    error: Option<Error>,
) -> Result<String, Error> {
    let diagnostic = error.map(|e| format!("{e:?}")).unwrap_or_default();
    let result = format!(
        "{{\"revision\":{revision},\"delivery\":{delivery},\"refreshExhausted\":{refresh_exhausted},\"sample\":{},\"width\":{},\"size\":{},\"direction\":{},\"alignment\":{},\"mode\":{},\"payload\":\"{}\",\"source\":\"{}\",\"fontSource\":\"{}\",\"imageWidth\":{},\"imageHeight\":{},\"pixelHash\":{},\"missing\":{},\"lineCount\":{},\"runCount\":{},\"geometry\":{},\"diagnostic\":\"{}\",\"io\":{{\"audio\":false,\"input\":false,\"midi\":false}}}}",
        state.sample,
        state.width,
        state.size,
        usize::from(state.rtl),
        state.alignment,
        usize::from(state.raster),
        p.payload,
        p.source,
        base64(name.as_bytes()),
        p.width,
        p.height,
        p.pixel_hash,
        p.missing,
        p.lines,
        p.runs,
        p.geometry,
        diagnostic
    );
    if result.len() > 65536 {
        Err(Error::Capacity)
    } else {
        Ok(result)
    }
}
#[cfg(any(all(target_os = "macos", feature = "live-ui"), test))]
fn command(json: &str) -> Option<(&str, usize, u32)> {
    if json.len() > 128 {
        return None;
    }
    let inner = json.strip_prefix('{')?.strip_suffix('}')?;
    let fields = inner.split(',').collect::<Vec<_>>();
    if fields.len() != 2 {
        return None;
    }
    let mut revision = None;
    let mut edit = None;
    for field in fields {
        let (k, v) = field.trim().split_once(':')?;
        let key = k.trim().strip_prefix('"')?.strip_suffix('"')?;
        let value = v.trim();
        if value.is_empty()
            || !value.bytes().all(|b| b.is_ascii_digit())
            || (value.len() > 1 && value.starts_with('0'))
        {
            return None;
        }
        if key == "revision" {
            if revision.is_some() {
                return None;
            }
            revision = Some(value.parse::<u32>().ok()?);
        } else {
            if edit.is_some() {
                return None;
            }
            edit = Some((key, value.parse::<usize>().ok()?));
        }
    }
    let (key, value) = edit?;
    Some((key, value, revision?))
}
pub fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for b in bytes.chunks(3) {
        let a = b[0];
        let c = b.get(1).copied().unwrap_or(0);
        let d = b.get(2).copied().unwrap_or(0);
        for v in [
            T[(a >> 2) as usize],
            T[(((a & 3) << 4) | (c >> 4)) as usize],
            if b.len() > 1 {
                T[(((c & 15) << 2) | (d >> 6)) as usize]
            } else {
                b'='
            },
            if b.len() > 2 {
                T[(d & 63) as usize]
            } else {
                b'='
            },
        ] {
            out.push(v as char);
        }
    }
    out
}
#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
