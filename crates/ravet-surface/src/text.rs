//! Optional local-font single-run shaping on UI/worker threads.
//! Font loading/shaping allocates and may run backend work beyond output limits;
//! neither belongs in an audio callback. No font discovery/network access,
//! fallback or native UI is added by shaping. The separate `raster` feature can
//! explicitly render prepared local-font outlines. Independent runs do not wrap;
//! [`crate::paragraph`] supplies explicit bounded wrapping. Single-line bidi
//! ordering is available separately in [`crate::bidi`].
use harfrust::{
    Buffer, BufferFlags, Direction, Feature, Font, Language, Script, ShapeOptions, ShaperFont, Tag,
};
/// Font bytes accepted before copying/parsing.
pub const MAXIMUM_FONT_BYTES: usize = 16 * 1024 * 1024;
/// UTF8 bytes per independent horizontal run.
pub const MAXIMUM_TEXT_BYTES: usize = 4096;
/// Returned glyphs; backend transient memory/time is not globally bounded here.
pub const MAXIMUM_SHAPED_GLYPHS: usize = 16384;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextError {
    InvalidInput,
    Capacity,
    UnsupportedFont,
    Shaping,
}
impl std::fmt::Display for TextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidInput => "invalid text shaping input",
            Self::Capacity => "text/font capacity exceeded",
            Self::UnsupportedFont => "unsupported font/face",
            Self::Shaping => "text shaping failed",
        })
    }
}
impl std::error::Error for TextError {}
/// Direction belongs to the caller's script/bidi segmentation, not auto detection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextDirection {
    LeftToRight,
    RightToLeft,
}
/// Explicit global OpenType feature override; printable ASCII tag, max 32/run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextFeature {
    pub tag: [u8; 4],
    pub enabled: bool,
}
/// Single horizontal run options. Size is finite (0,1024] caller-selected units.
/// Script may be guessed from the run; language is an optional ASCII alphanumeric/
/// hyphen tag of 1..63 bytes. This does not perform full BCP47 validation.
#[derive(Clone, Copy, Debug)]
pub struct TextOptions<'a> {
    pub size: f64,
    pub direction: TextDirection,
    pub script: Option<[u8; 4]>,
    pub language: Option<&'a str>,
    pub features: &'a [TextFeature],
}
/// Positioned glyph in visual order, with UTF8 byte cluster and scaled font-unit
/// advances/offsets. Y points up; renderer owns any baseline/coordinate transform.
/// Glyph zero is the missing-glyph result; no fallback face is selected.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapedGlyph {
    pub id: u32,
    pub cluster: u32,
    pub x: f64,
    pub y: f64,
    pub x_advance: f64,
    pub y_advance: f64,
}
/// Owned shaped run. Advance is pen movement, rather than glyph ink bounds.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapedText {
    glyphs: Vec<ShapedGlyph>,
    advance: (f64, f64),
    direction: TextDirection,
}
impl ShapedText {
    pub fn glyphs(&self) -> &[ShapedGlyph] {
        &self.glyphs
    }
    pub fn advance(&self) -> (f64, f64) {
        self.advance
    }
    pub fn direction(&self) -> TextDirection {
        self.direction
    }
    pub fn missing_glyphs(&self) -> usize {
        self.glyphs.iter().filter(|glyph| glyph.id == 0).count()
    }
}
/// Owned local font at its default variation instance. Parsing/caching may allocate.
/// Backend-supported formats/faces only; parsing is not full corruption validation.
/// The consumer owns font licensing, fallback selection and rendered font identity.
#[derive(Clone)]
pub struct TextFont {
    font: Font,
}
impl TextFont {
    /// Copy at most 16 MiB of local font data and select its face index. No files,
    /// system fonts, network or global registration are consulted.
    pub fn from_bytes(data: &[u8], face_index: u32) -> Result<Self, TextError> {
        if data.len() > MAXIMUM_FONT_BYTES {
            return Err(TextError::Capacity);
        }
        if data.is_empty() {
            return Err(TextError::UnsupportedFont);
        }
        let font = Font::new(data.to_vec(), face_index).ok_or(TextError::UnsupportedFont)?;
        if font.units_per_em() == 0 || font.num_glyphs() == 0 {
            return Err(TextError::UnsupportedFont);
        }
        Ok(Self { font })
    }
    /// Extract one default-instance TrueType glyf outline as typed font-unit
    /// paths, at most 4096 commands. Empty glyphs yield no commands. CFF/Type1/
    /// color/bitmap rendering is unsupported; invalid IDs reject. Backend loads
    /// can allocate before output limits, so this belongs on UI/worker threads.
    pub fn outline_glyph(&self, id: u32) -> Result<Vec<crate::drawing::PathCommand>, TextError> {
        use read_fonts::{
            TableProvider,
            tables::glyf::outline::{Outline, PathContourStart, Unscaled},
        };
        if id >= self.font.num_glyphs() {
            return Err(TextError::InvalidInput);
        }
        if self.font.tables().glyf().is_err() {
            return Err(TextError::UnsupportedFont);
        }
        let mut outline = Outline::<Unscaled>::new();
        let context = &self.font;
        let loaded = outline
            .load_with(&context, harfrust::GlyphId::new(id), &Unscaled, None)
            .map_err(|_| TextError::Shaping)?;
        let mut pen = BoundedOutlinePen {
            commands: Vec::new(),
            overflow: false,
            invalid: false,
        };
        if !loaded.to_path(PathContourStart::ScanBackward, &mut pen) {
            return Err(TextError::Shaping);
        }
        if pen.invalid {
            return Err(TextError::Shaping);
        }
        if pen.overflow {
            return Err(TextError::Capacity);
        }
        Ok(pen.commands)
    }
    pub fn units_per_em(&self) -> u16 {
        self.font.units_per_em()
    }
    /// Shape one complete horizontal run with default font features plus explicit
    /// global overrides. Reject controls/newlines and Unicode noncharacters;
    /// ordinary spaces, combining marks and joiners remain permitted. The caller
    /// owns script/bidi segmentation/context and caches returned immutable runs.
    pub fn shape(&self, text: &str, options: TextOptions<'_>) -> Result<ShapedText, TextError> {
        if text.len() > MAXIMUM_TEXT_BYTES || options.features.len() > 32 {
            return Err(TextError::Capacity);
        }
        if !options.size.is_finite()
            || options.size <= 0.
            || options.size > 1024.
            || !valid_single_line(text)
            || options
                .features
                .iter()
                .any(|feature| feature.tag.iter().any(|c| !(32..=126).contains(c)))
        {
            return Err(TextError::InvalidInput);
        }
        if let Some(language) = options.language
            && (language.is_empty()
                || language.len() > 63
                || !language
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-'))
        {
            return Err(TextError::InvalidInput);
        }
        let script = options
            .script
            .map(|tag| Script::from_iso15924_tag(Tag::new(&tag)).ok_or(TextError::InvalidInput))
            .transpose()?;
        let mut buffer = Buffer::new();
        buffer.push_str(text);
        buffer.set_direction(match options.direction {
            TextDirection::LeftToRight => Direction::LeftToRight,
            TextDirection::RightToLeft => Direction::RightToLeft,
        });
        buffer.set_script(script);
        if let Some(language) = options.language {
            buffer.set_language(Some(
                language
                    .parse::<Language>()
                    .map_err(|_| TextError::InvalidInput)?,
            ));
        }
        buffer.set_flags(BufferFlags::BEGINNING_OF_TEXT | BufferFlags::END_OF_TEXT);
        buffer.guess_segment_properties();
        let features: Vec<_> = options
            .features
            .iter()
            .map(|feature| Feature::new(Tag::new(&feature.tag), u32::from(feature.enabled), ..))
            .collect();
        harfrust::shape(
            &ShaperFont::new(&self.font),
            &mut buffer,
            ShapeOptions::default().features(&features),
        )
        .map_err(|_| TextError::Shaping)?;
        if buffer.glyph_infos().len() > MAXIMUM_SHAPED_GLYPHS {
            return Err(TextError::Capacity);
        }
        let scale = options.size / f64::from(self.units_per_em());
        let mut pen = (0., 0.);
        let glyphs = buffer
            .glyph_infos()
            .iter()
            .zip(buffer.glyph_positions())
            .map(|(info, position)| {
                let glyph = ShapedGlyph {
                    id: info.glyph_id,
                    cluster: info.cluster,
                    x: pen.0 + f64::from(position.x_offset) * scale,
                    y: pen.1 + f64::from(position.y_offset) * scale,
                    x_advance: f64::from(position.x_advance) * scale,
                    y_advance: f64::from(position.y_advance) * scale,
                };
                pen.0 += glyph.x_advance;
                pen.1 += glyph.y_advance;
                glyph
            })
            .collect();
        Ok(ShapedText {
            glyphs,
            advance: pen,
            direction: options.direction,
        })
    }
}

struct BoundedOutlinePen {
    commands: Vec<crate::drawing::PathCommand>,
    overflow: bool,
    invalid: bool,
}
impl BoundedOutlinePen {
    fn push(&mut self, command: crate::drawing::PathCommand) {
        if !command.coordinates_are_finite() {
            self.invalid = true;
            return;
        }
        if self.commands.len() == 4096 {
            self.overflow = true;
        } else if !self.overflow {
            self.commands.push(command);
        }
    }
}
impl read_fonts::model::glyph::outline::OutlinePen for BoundedOutlinePen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.push(crate::drawing::PathCommand::MoveTo(
            f64::from(x),
            f64::from(y),
        ));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.push(crate::drawing::PathCommand::LineTo(
            f64::from(x),
            f64::from(y),
        ));
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.push(crate::drawing::PathCommand::QuadraticTo {
            control: (f64::from(cx), f64::from(cy)),
            end: (f64::from(x), f64::from(y)),
        });
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.push(crate::drawing::PathCommand::CubicTo {
            first: (f64::from(cx0), f64::from(cy0)),
            second: (f64::from(cx1), f64::from(cy1)),
            end: (f64::from(x), f64::from(y)),
        });
    }
    fn close(&mut self) {
        self.push(crate::drawing::PathCommand::Close);
    }
}

pub(crate) fn valid_single_line(text: &str) -> bool {
    !text.chars().any(|c| {
        c.is_control()
            || matches!(c, '\u{2028}' | '\u{2029}')
            || (0xfdd0..=0xfdef).contains(&(c as u32))
            || c as u32 & 0xffff >= 0xfffe
    })
}

// Shared paragraph/editing control policy.
pub(crate) fn valid_paragraph(text: &str) -> bool {
    text.chars().all(|c| {
        matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
            || valid_single_line(c.encode_utf8(&mut [0; 4]))
    })
}
