//! Bounded normal consumer: exact selection/render publication, independent paged presentation.
use ravet_surface::{
    drawing::{Color, SvgDrawing},
    raster::{RasterCanvas, RasterError, RasterText},
    text::*,
};
use ravet_webview::fonts::*;
use std::fmt::Write;
pub trait FontSource {
    fn catalog(&self) -> &FontCatalog;
    fn load(&self, selection: FontSelection) -> Result<TextFont, FontError>;
}
#[cfg(target_os = "macos")]
impl FontSource for SystemFontCatalog {
    fn catalog(&self) -> &FontCatalog {
        self.catalog()
    }
    fn load(&self, selection: FontSelection) -> Result<TextFont, FontError> {
        let loaded = self.load(selection)?;
        let font = TextFont::from_bytes(loaded.bytes(), 0).map_err(text_error)?;
        let settings: Vec<_> = loaded
            .variations()
            .iter()
            .map(|v| TextVariation {
                tag: v.tag,
                value: v.value,
            })
            .collect();
        font.with_variations(&settings).map_err(text_error)
    }
}
fn text_error(e: TextError) -> FontError {
    if matches!(e, TextError::Capacity) {
        FontError::Capacity
    } else {
        FontError::UnsupportedFont
    }
}
fn raster_error(error: RasterError) -> FontError {
    match error {
        RasterError::Capacity => FontError::Capacity,
        RasterError::Text(error) => text_error(error),
        RasterError::InvalidInput => FontError::UnsupportedFont,
    }
}
pub const SAMPLES: [&str; 2] = ["Agam0", "A g m 0"];
pub const SIZES: [f64; 3] = [14., 20., 28.];
const PAGE: usize = 4;
pub struct Model<S: FontSource> {
    source: S,
    font: TextFont,
    selected: FontSelection,
    family: usize,
    family_page: usize,
    style_page: usize,
    sample: usize,
    size: usize,
    raster: bool,
    revision: u32,
    diagnostic: Option<FontError>,
    payload: String,
    advance: f64,
    pub snapshot: String,
}
impl<S: FontSource> Model<S> {
    pub fn new(source: S, initial: FontSelection) -> Result<Self, FontError> {
        let descriptor = source.catalog().descriptor(initial)?;
        let families = source.catalog().families();
        let family = families
            .iter()
            .position(|f| *f == descriptor.family())
            .ok_or(FontError::InvalidInput)?;
        let styles = source.catalog().styles(descriptor.family());
        let style = styles
            .iter()
            .position(|s| *s == initial)
            .ok_or(FontError::InvalidInput)?;
        let font = source.load(initial)?;
        let (payload, advance) = render(&font, 0, 0, false)?;
        let mut model = Self {
            source,
            font,
            selected: initial,
            family,
            family_page: family / PAGE,
            style_page: style / PAGE,
            sample: 0,
            size: 0,
            raster: false,
            revision: 0,
            diagnostic: None,
            payload,
            advance,
            snapshot: String::new(),
        };
        model.snapshot = model.prepare()?;
        Ok(model)
    }
    pub fn catalog(&self) -> &FontCatalog {
        self.source.catalog()
    }
    pub fn selected(&self) -> FontSelection {
        self.selected
    }
    pub fn revision(&self) -> u32 {
        self.revision
    }
    pub fn payload(&self) -> &str {
        &self.payload
    }
    pub fn diagnostic(&self) -> Option<FontError> {
        self.diagnostic
    }
    fn prepare(&self) -> Result<String, FontError> {
        let catalog = self.source.catalog();
        let families = catalog.families();
        let desc = catalog.descriptor(self.selected)?;
        let styles = catalog.styles(families[self.family]);
        let mut s = format!(
            "{{\"revision\":{},\"selected\":{},\"postscript\":{},\"familyCount\":{},\"familyPage\":{},\"browseFamily\":{},\"styleCount\":{},\"stylePage\":{},\"sample\":{},\"size\":{},\"mode\":{},\"advance\":{},\"payload\":{},\"diagnostic\":{},\"families\":[",
            self.revision,
            index(catalog, self.selected)?,
            quote(desc.postscript_name()),
            families.len(),
            self.family_page,
            self.family,
            styles.len(),
            self.style_page,
            self.sample,
            self.size,
            quote(if self.raster { "raster" } else { "svg" }),
            self.advance,
            quote(&self.payload),
            self.diagnostic
                .map_or("null".into(), |e| quote(&format!("{e:?}")))
        );
        for (i, family) in families
            .iter()
            .enumerate()
            .skip(self.family_page * PAGE)
            .take(PAGE)
        {
            if !s.ends_with('[') {
                s.push(',')
            }
            write!(s, "{{\"id\":{i},\"name\":{}}}", quote(family)).unwrap();
        }
        s.push_str("],\"styles\":[");
        for selection in styles.iter().skip(self.style_page * PAGE).take(PAGE) {
            if !s.ends_with('[') {
                s.push(',')
            }
            let d = catalog.descriptor(*selection)?;
            write!(
                s,
                "{{\"id\":{},\"name\":{},\"postscript\":{},\"unsupported\":{}}}",
                index(catalog, *selection)?,
                quote(d.style()),
                quote(d.postscript_name()),
                d.diagnostic()
                    .map_or("null".into(), |e| quote(&format!("{e:?}")))
            )
            .unwrap();
        }
        s.push_str("],\"coordinates\":[");
        for c in self.font.variation_coordinates().map_err(text_error)? {
            if !s.ends_with('[') {
                s.push(',')
            }
            write!(
                s,
                "{{\"tag\":{},\"user\":{},\"normalized\":{}}}",
                quote(std::str::from_utf8(&c.tag).map_err(|_| FontError::UnsupportedFont)?),
                c.user,
                c.normalized
            )
            .unwrap();
        }
        s.push_str("],\"io\":{\"audio\":false,\"input\":false,\"midi\":false}}");
        if s.len() > 65536 {
            return Err(FontError::Capacity);
        }
        Ok(s)
    }
    /// Commands use exact indices and the currently displayed revision; no arbitrary font name/path.
    pub fn message(&mut self, json: &str) -> bool {
        if json.len() > 128 {
            return false;
        }
        let Some((key, value, revision)) = command(json) else {
            return false;
        };
        if revision != self.revision {
            return false;
        }
        if key == "font" {
            let Ok(selection) = self.catalog().selection(value) else {
                return false;
            };
            let families = self.catalog().families();
            let styles = self.catalog().styles(families[self.family]);
            if !styles
                .iter()
                .skip(self.style_page * PAGE)
                .take(PAGE)
                .any(|s| *s == selection)
            {
                return false;
            }
            return self.select(selection).is_ok();
        }
        let old = (
            self.family,
            self.family_page,
            self.style_page,
            self.sample,
            self.size,
            self.raster,
        );
        let families = self.catalog().families();
        let styles = self.catalog().styles(families[self.family]);
        match key {
            "family" => {
                if !((self.family_page * PAGE)..((self.family_page + 1) * PAGE).min(families.len()))
                    .contains(&value)
                {
                    return false;
                }
                self.family = value;
                self.style_page = 0;
            }
            "familyPage" => {
                if value >= families.len().div_ceil(PAGE) {
                    return false;
                }
                self.family_page = value;
            }
            "stylePage" => {
                if value >= styles.len().div_ceil(PAGE) {
                    return false;
                }
                self.style_page = value;
            }
            "sample" => {
                if value >= SAMPLES.len() {
                    return false;
                }
                self.sample = value;
            }
            "size" => {
                if value >= SIZES.len() {
                    return false;
                }
                self.size = value;
            }
            "mode" => {
                if value > 1 {
                    return false;
                }
                self.raster = value == 1;
            }
            _ => return false,
        }
        let new = (
            self.family,
            self.family_page,
            self.style_page,
            self.sample,
            self.size,
            self.raster,
        );
        if old == new {
            return true;
        }
        let render_change = old.3 != new.3 || old.4 != new.4 || old.5 != new.5;
        let prepared = if render_change {
            render(&self.font, self.sample, self.size, self.raster)
        } else {
            Ok((self.payload.clone(), self.advance))
        };
        let old_revision = self.revision;
        let old_diagnostic = self.diagnostic;
        let old_payload = self.payload.clone();
        let old_advance = self.advance;
        let accepted = (|| {
            let (payload, advance) = prepared?;
            self.revision = self.revision.checked_add(1).ok_or(FontError::Capacity)?;
            self.payload = payload;
            self.advance = advance;
            self.diagnostic = None;
            self.prepare()
        })();
        match accepted {
            Ok(snapshot) => {
                self.snapshot = snapshot;
                true
            }
            Err(_) => {
                (
                    self.family,
                    self.family_page,
                    self.style_page,
                    self.sample,
                    self.size,
                    self.raster,
                ) = old;
                self.revision = old_revision;
                self.diagnostic = old_diagnostic;
                self.payload = old_payload;
                self.advance = old_advance;
                false
            }
        }
    }
    /// Direct selection supports an exact catalogue token. Failed requests preserve admitted font/image.
    pub fn select(&mut self, selection: FontSelection) -> Result<(), FontError> {
        self.catalog().descriptor(selection)?;
        if selection == self.selected && self.diagnostic.is_none() {
            return Ok(());
        }
        let prepared = self.source.load(selection).and_then(|font| {
            render(&font, self.sample, self.size, self.raster)
                .map(|(payload, advance)| (font, payload, advance))
        });
        let next = self.revision.checked_add(1).ok_or(FontError::Capacity)?;
        match prepared {
            Err(error) => {
                let prior = self.diagnostic;
                let revision = self.revision;
                self.diagnostic = Some(error);
                self.revision = next;
                match self.prepare() {
                    Ok(snapshot) => self.snapshot = snapshot,
                    Err(e) => {
                        self.diagnostic = prior;
                        self.revision = revision;
                        return Err(e);
                    }
                }
                Err(error)
            }
            Ok((font, payload, advance)) => {
                let old = (
                    self.font.clone(),
                    self.selected,
                    self.payload.clone(),
                    self.advance,
                    self.revision,
                    self.diagnostic,
                );
                self.font = font;
                self.selected = selection;
                self.payload = payload;
                self.advance = advance;
                self.revision = next;
                self.diagnostic = None;
                match self.prepare() {
                    Ok(snapshot) => {
                        self.snapshot = snapshot;
                        Ok(())
                    }
                    Err(e) => {
                        (
                            self.font,
                            self.selected,
                            self.payload,
                            self.advance,
                            self.revision,
                            self.diagnostic,
                        ) = old;
                        Err(e)
                    }
                }
            }
        }
    }
}
fn index(catalog: &FontCatalog, selection: FontSelection) -> Result<usize, FontError> {
    (0..catalog.entries().len())
        .find(|i| catalog.selection(*i) == Ok(selection))
        .ok_or(FontError::ForeignSelection)
}
fn render(
    font: &TextFont,
    sample: usize,
    size: usize,
    raster: bool,
) -> Result<(String, f64), FontError> {
    let source = *SAMPLES.get(sample).ok_or(FontError::InvalidInput)?;
    let size = *SIZES.get(size).ok_or(FontError::InvalidInput)?;
    if source.len() > 64 {
        return Err(FontError::Capacity);
    }
    let shaped = font
        .shape(
            source,
            TextOptions {
                size,
                direction: TextDirection::LeftToRight,
                script: Some(*b"Latn"),
                language: Some("en"),
                features: &[],
            },
        )
        .map_err(text_error)?;
    if shaped.missing_glyphs() != 0 {
        return Err(FontError::UnsupportedFont);
    }
    let payload = if raster {
        let text = RasterText::from_shaped(font, &shaped, size, (4., 36.)).map_err(raster_error)?;
        let mut canvas = RasterCanvas::new(128, 48, 1).map_err(|_| FontError::Capacity)?;
        canvas.clear(Color::rgb(255, 255, 255));
        canvas
            .draw_text(&text, Color::rgb(32, 62, 96), true)
            .map_err(raster_error)?;
        base64(canvas.pixels())
    } else {
        let mut svg = SvgDrawing::new(128., 48., source).map_err(|_| FontError::InvalidInput)?;
        svg.shaped_text(
            font,
            source,
            TextOptions {
                size,
                direction: TextDirection::LeftToRight,
                script: Some(*b"Latn"),
                language: Some("en"),
                features: &[],
            },
            (4., 36.),
            Color::rgb(32, 62, 96),
        )
        .map_err(text_error)?;
        let s = svg.finish();
        if s.len() > 32768 {
            return Err(FontError::Capacity);
        }
        base64(s.as_bytes())
    };
    Ok((payload, shaped.advance().0))
}
fn command(json: &str) -> Option<(&str, usize, u32)> {
    let body = json.strip_prefix('{')?.strip_suffix('}')?;
    let mut pairs = body.split(',');
    let first = pairs.next()?;
    let second = pairs.next()?;
    if pairs.next().is_some() {
        return None;
    }
    fn pair(s: &str) -> Option<(&str, &str)> {
        let (k, v) = s.split_once(':')?;
        let k = k.strip_prefix('"')?.strip_suffix('"')?;
        if v.is_empty()
            || !v.bytes().all(|b| b.is_ascii_digit())
            || (v.len() > 1 && v.starts_with('0'))
        {
            return None;
        }
        Some((k, v))
    }
    let a = pair(first)?;
    let b = pair(second)?;
    let (command, revision) = if a.0 == "revision" && b.0 != "revision" {
        (b, a.1)
    } else if b.0 == "revision" && a.0 != "revision" {
        (a, b.1)
    } else {
        return None;
    };
    Some((command.0, command.1.parse().ok()?, revision.parse().ok()?))
}
pub fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => write!(out, "\\u{:04x}", c as u32).unwrap(),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
pub fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for b in bytes.chunks(3) {
        let a = b[0];
        let c = b.get(1).copied().unwrap_or(0);
        let d = b.get(2).copied().unwrap_or(0);
        for x in [
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
            out.push(x as char)
        }
    }
    out
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
