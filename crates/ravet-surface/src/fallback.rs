//! Caller-owned local font fallback for one explicit script/directional run.
//!
//! This allocating UI/worker facility probes whole Unicode 17 extended graphemes,
//! retains original surrounding shaping context and selects only supplied fonts.
//! No system lookup, registration, mixed bidi, paragraph layout or audio work.
use crate::text::{
    MAXIMUM_SHAPED_GLYPHS, MAXIMUM_TEXT_BYTES, ShapedText, TextDirection, TextError, TextFeature,
    TextFont, TextOptions,
};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;
/// Maximum ordered caller-supplied faces; each retains its existing 16 MiB limit.
pub const MAXIMUM_FALLBACK_FONTS: usize = 16;
/// Maximum complete merged output font runs.
pub const MAXIMUM_FALLBACK_RUNS: usize = 256;
/// Maximum backend grapheme probes before whole Capacity failure.
pub const MAXIMUM_FALLBACK_PROBES: usize = 8192;

/// Copied ordered references. The actual fonts remain consumer-owned.
pub struct TextFontStack<'f> {
    fonts: Vec<&'f TextFont>,
}
impl<'f> TextFontStack<'f> {
    /// Borrow 1..16 faces in priority order; no implicit resources or deduplication.
    pub fn new(fonts: &[&'f TextFont]) -> Result<Self, TextError> {
        if fonts.is_empty() {
            return Err(TextError::InvalidInput);
        }
        if fonts.len() > MAXIMUM_FALLBACK_FONTS {
            return Err(TextError::Capacity);
        }
        Ok(Self {
            fonts: fonts.to_vec(),
        })
    }
    pub fn fonts(&self) -> &[&'f TextFont] {
        &self.fonts
    }
    /// Shape <=4096 UTF8 bytes, explicit four-ASCII-letter script/direction and existing text options.
    /// Select the first font that shapes each complete extended grapheme without
    /// glyph zero, using original context; adjacent same-face units shape together.
    /// Default ignorables participate in shaping then disappear from glyph output.
    /// Missing units use the primary replacement and retain explicit source ranges.
    /// Final unexpected missing glyphs for covered source reject the whole result.
    /// Output <=256 runs/16384 glyphs; probe count <=8192. Backend transient memory/time
    /// is not globally bounded. No cross-font ligatures/presentation guarantee.
    pub fn shape(
        &self,
        text: &str,
        options: TextOptions<'_>,
    ) -> Result<FallbackText<'f>, TextError> {
        if text.len() > MAXIMUM_TEXT_BYTES {
            return Err(TextError::Capacity);
        }
        if options
            .script
            .is_none_or(|tag| tag.iter().any(|byte| !byte.is_ascii_alphabetic()))
        {
            return Err(TextError::InvalidInput);
        }
        self.fonts[0].shape_contextual(text, 0..0, options, true)?;
        let mut logical: Vec<(Range<usize>, usize)> = Vec::new();
        let mut missing = Vec::new();
        let mut probes = 0;
        for (start, grapheme) in text.grapheme_indices(true) {
            let range = start..start + grapheme.len();
            let chosen = choose_font(self.fonts(), text, range.clone(), options, || {
                if probes == MAXIMUM_FALLBACK_PROBES {
                    return Err(TextError::Capacity);
                }
                probes += 1;
                Ok(())
            })?;
            if chosen.is_none() {
                missing.push(range.clone());
            }
            let index = chosen.unwrap_or(0);
            if let Some((last, previous)) = logical.last_mut()
                && *previous == index
            {
                last.end = range.end;
            } else {
                if logical.len() == MAXIMUM_FALLBACK_RUNS {
                    return Err(TextError::Capacity);
                }
                logical.push((range, index));
            }
        }
        if options.direction == TextDirection::RightToLeft {
            logical.reverse();
        }
        let mut runs = Vec::with_capacity(logical.len());
        let mut pen = (0., 0.);
        let mut glyph_count = 0;
        let mut missing_count = 0;
        for (range, index) in logical {
            let font = self.fonts[index];
            let shape = font.shape_contextual(text, range.clone(), options, true)?;
            if shape.glyphs().len() > MAXIMUM_SHAPED_GLYPHS - glyph_count {
                return Err(TextError::Capacity);
            }
            glyph_count += shape.glyphs().len();
            missing_count += validate_coverage(text, &range, &shape, &missing)?;
            let advance = shape.advance();
            let origin = pen;
            pen.0 += advance.0;
            pen.1 += advance.1;
            if !pen.0.is_finite() || !pen.1.is_finite() {
                return Err(TextError::Shaping);
            }
            runs.push(FallbackRun {
                range,
                font_index: index,
                font,
                shape,
                origin,
            });
        }
        Ok(FallbackText {
            text: text.to_owned(),
            script: options.script.expect("checked explicit script"),
            language: options.language.map(str::to_owned),
            features: options.features.to_vec(),
            size: options.size,
            direction: options.direction,
            runs,
            missing,
            glyph_count,
            missing_count,
            advance: pen,
            probes,
        })
    }
}
/// Immutable visual font run retaining the exact matching face.
pub struct FallbackRun<'f> {
    range: Range<usize>,
    font_index: usize,
    font: &'f TextFont,
    shape: ShapedText,
    origin: (f64, f64),
}
impl<'f> FallbackRun<'f> {
    pub fn range(&self) -> Range<usize> {
        self.range.clone()
    }
    pub fn font_index(&self) -> usize {
        self.font_index
    }
    pub fn font(&self) -> &'f TextFont {
        self.font
    }
    pub fn shape(&self) -> &ShapedText {
        &self.shape
    }
    /// Local upward-Y shaping pen placement; renderer owns baseline conversion.
    pub fn origin(&self) -> (f64, f64) {
        self.origin
    }
    pub fn source_cluster(&self, glyph_index: usize) -> Option<usize> {
        self.shape
            .glyphs()
            .get(glyph_index)
            .map(|g| self.range.start + g.cluster as usize)
    }
}
/// Complete immutable source/matched-face output. Cannot outlive supplied fonts.
/// Glyph clusters preserve source bytes; ignorable source remains even without glyphs.
///
/// ```compile_fail
/// use ravet_surface::{fallback::TextFontStack, text::{TextFont,TextOptions}};
/// fn escape<'a>(bytes:&[u8],options:TextOptions<'_>) -> ravet_surface::fallback::FallbackText<'a> {
///     let font=TextFont::from_bytes(bytes,0).unwrap();
///     TextFontStack::new(&[&font]).unwrap().shape("hello",options).unwrap()
/// }
/// ```
pub struct FallbackText<'f> {
    text: String,
    script: [u8; 4],
    language: Option<String>,
    features: Vec<TextFeature>,
    size: f64,
    direction: TextDirection,
    runs: Vec<FallbackRun<'f>>,
    missing: Vec<Range<usize>>,
    glyph_count: usize,
    missing_count: usize,
    advance: (f64, f64),
    probes: usize,
}
impl<'f> FallbackText<'f> {
    pub fn text(&self) -> &str {
        &self.text
    }
    /// Copied shaping style used for probes/final output, independent of caller buffers.
    pub fn options(&self) -> TextOptions<'_> {
        TextOptions {
            size: self.size,
            direction: self.direction,
            script: Some(self.script),
            language: self.language.as_deref(),
            features: &self.features,
        }
    }
    pub fn size(&self) -> f64 {
        self.size
    }
    pub fn direction(&self) -> TextDirection {
        self.direction
    }
    pub fn runs(&self) -> &[FallbackRun<'f>] {
        &self.runs
    }
    /// Unresolved complete logical graphemes, sorted in original source order.
    pub fn missing_ranges(&self) -> &[Range<usize>] {
        &self.missing
    }
    pub fn glyph_count(&self) -> usize {
        self.glyph_count
    }
    pub fn missing_glyphs(&self) -> usize {
        self.missing_count
    }
    pub fn advance(&self) -> (f64, f64) {
        self.advance
    }
    pub fn probes(&self) -> usize {
        self.probes
    }
}

// Shared coverage policy for single-run and paragraph fallback. Charging happens
// before backend work; callers own their whole-operation counters.
pub(crate) fn choose_font(
    fonts: &[&TextFont],
    context: &str,
    range: Range<usize>,
    options: TextOptions<'_>,
    mut charge: impl FnMut() -> Result<(), TextError>,
) -> Result<Option<usize>, TextError> {
    for (index, font) in fonts.iter().enumerate() {
        charge()?;
        if font
            .shape_contextual(context, range.clone(), options, true)?
            .missing_glyphs()
            == 0
        {
            return Ok(Some(index));
        }
    }
    Ok(None)
}
pub(crate) fn validate_coverage(
    source: &str,
    range: &Range<usize>,
    shape: &ShapedText,
    missing: &[Range<usize>],
) -> Result<usize, TextError> {
    let mut count = 0;
    for glyph in shape.glyphs() {
        let cluster = range
            .start
            .checked_add(glyph.cluster as usize)
            .ok_or(TextError::Shaping)?;
        if cluster >= range.end
            || !source.is_char_boundary(cluster)
            || [glyph.x, glyph.y, glyph.x_advance, glyph.y_advance]
                .iter()
                .any(|v| !v.is_finite())
        {
            return Err(TextError::Shaping);
        }
        if glyph.id == 0 {
            if !missing.iter().any(|r| r.contains(&cluster)) {
                return Err(TextError::Shaping);
            }
            count += 1;
        }
    }
    Ok(count)
}
