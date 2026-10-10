//! Bounded local-font paragraph wrapping/shaping on UI or worker threads.
//!
//! Line opportunities use pinned Unicode 15.0 data; paragraph bidi uses pinned
//! Unicode 16.0 data. This is explicit single-font layout, not system fallback,
//! dictionary breaking, discretionary hyphen insertion, justification or interactive input.
use crate::text::{
    MAXIMUM_SHAPED_GLYPHS, MAXIMUM_TEXT_BYTES, ShapedText, TextDirection, TextError, TextFont,
    TextOptions, valid_paragraph,
};
use std::ops::Range;
use unicode_linebreak::{BreakOpportunity, linebreaks};

/// Maximum complete output lines, including empty lines after hard separators.
pub const MAXIMUM_PARAGRAPH_LINES: usize = 256;
/// Maximum complete output directional runs across all lines.
pub const MAXIMUM_PARAGRAPH_RUNS: usize = 512;

/// Physical horizontal alignment; independent of paragraph writing direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAlignment {
    Left,
    Center,
    Right,
}
/// Explicit finite positive width and baseline spacing, in shaping size units.
/// Whitespace is retained, so trailing-space advance participates in wrapping.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParagraphOptions {
    pub width: f64,
    pub line_height: f64,
    pub alignment: TextAlignment,
}
/// One visual run; glyph clusters remain relative to its original logical slice.
/// Glyph positions use run-local coordinates. Place the run after its line's
/// alignment/baseline offset and this run's horizontal offset.
#[derive(Clone, Debug, PartialEq)]
pub struct ParagraphRun {
    range: Range<usize>,
    shape: ShapedText,
    x: f64,
    level: u8,
}
impl ParagraphRun {
    pub fn range(&self) -> Range<usize> {
        self.range.clone()
    }
    pub fn shape(&self) -> &ShapedText {
        &self.shape
    }
    pub fn x(&self) -> f64 {
        self.x
    }
    pub fn level(&self) -> u8 {
        self.level
    }
    /// Original paragraph UTF8 byte offset for the selected glyph's cluster.
    pub fn source_cluster(&self, glyph_index: usize) -> Option<usize> {
        self.shape
            .glyphs()
            .get(glyph_index)
            .map(|glyph| self.range.start + glyph.cluster as usize)
    }
}
/// One logical line and its visual directional runs. The range excludes a hard
/// separator, whose original bytes are retained separately. Baseline Y increases
/// downwards, starting at zero; this is pen layout, not glyph ink bounds.
#[derive(Clone, Debug, PartialEq)]
pub struct ParagraphLine {
    range: Range<usize>,
    separator: Range<usize>,
    runs: Vec<ParagraphRun>,
    width: f64,
    baseline: (f64, f64),
    overflows: bool,
}
impl ParagraphLine {
    pub fn range(&self) -> Range<usize> {
        self.range.clone()
    }
    pub fn separator(&self) -> Range<usize> {
        self.separator.clone()
    }
    pub fn runs(&self) -> &[ParagraphRun] {
        &self.runs
    }
    pub fn width(&self) -> f64 {
        self.width
    }
    pub fn baseline(&self) -> (f64, f64) {
        self.baseline
    }
    /// An unbreakable unit may exceed the requested width; it is not split or clipped.
    pub fn overflows(&self) -> bool {
        self.overflows
    }
}
/// Owned immutable output retaining the original text, paragraph options and base
/// direction. Font/shaping style identity remains caller-owned.
/// Preparation allocates and repeatedly shapes candidate lines; backend transient
/// work is not bounded by output caps and must never run in audio callbacks.
#[derive(Clone, Debug, PartialEq)]
pub struct ParagraphLayout {
    text: String,
    options: ParagraphOptions,
    direction: TextDirection,
    lines: Vec<ParagraphLine>,
    glyph_count: usize,
    height: f64,
}
impl ParagraphLayout {
    /// Wrap <=4096 UTF8 bytes at Unicode line opportunities. Greedy fitting stops
    /// at the first overflowing candidate; an oversized first unbreakable unit
    /// overflows unchanged and is flagged. No character reversal or forced split.
    ///
    /// Preserve paragraph embedding/isolate context across soft wraps, applying
    /// bidi line reordering to each candidate before shaping visual runs. Base
    /// direction is explicit in `text_options`; script/language/features apply
    /// to each directional run and remain caller-owned segmentation policy.
    ///
    /// LF, CRLF, CR and Unicode line/paragraph separators create hard lines;
    /// empty input and a trailing separator produce an empty line. Other controls
    /// and noncharacters reject. Output caps are 256 lines/512 runs/16384 glyphs.
    /// Complete failure returns no partial layout. No fonts/files/network are discovered.
    pub fn new(
        font: &TextFont,
        text: &str,
        text_options: TextOptions<'_>,
        options: ParagraphOptions,
    ) -> Result<Self, TextError> {
        if text.len() > MAXIMUM_TEXT_BYTES {
            return Err(TextError::Capacity);
        }
        if !options.width.is_finite()
            || options.width <= 0.
            || !options.line_height.is_finite()
            || options.line_height <= 0.
            || !valid_paragraph(text)
        {
            return Err(TextError::InvalidInput);
        }
        // Validate global style even when every line is empty, before bidi/backend work.
        font.shape("", text_options)?;
        let base = match text_options.direction {
            TextDirection::LeftToRight => unicode_bidi::Level::ltr(),
            TextDirection::RightToLeft => unicode_bidi::Level::rtl(),
        };
        let bidi = unicode_bidi::BidiInfo::new(text, Some(base));
        let breaks: Vec<_> = linebreaks(text).collect();
        let mut lines = Vec::new();
        let mut start = 0;
        let mut cursor = 0;
        let mut glyph_count = 0;
        let mut run_count = 0;
        while start < text.len() {
            let mut selected = None;
            for (index, &(end, opportunity)) in breaks.iter().enumerate().skip(cursor) {
                let content_end = if opportunity == BreakOpportunity::Mandatory {
                    separator_start(text, start, end)
                } else {
                    end
                };
                let candidate = shape_line(font, text, &bidi, start..content_end, text_options)?;
                let overflow = candidate.width > options.width;
                if !overflow || selected.is_none() {
                    selected = Some((index, end, candidate));
                }
                if overflow || opportunity == BreakOpportunity::Mandatory {
                    break;
                }
            }
            let (index, end, mut line) = selected.ok_or(TextError::Shaping)?;
            if end <= start {
                return Err(TextError::Shaping);
            }
            line.separator = line.range.end..end;
            place_line(&mut line, lines.len(), options)?;
            append_line(&mut lines, line, &mut glyph_count, &mut run_count)?;
            start = end;
            cursor = index + 1;
        }
        if text.is_empty() || separator_start(text, 0, text.len()) < text.len() {
            let mut line = shape_line(font, text, &bidi, text.len()..text.len(), text_options)?;
            place_line(&mut line, lines.len(), options)?;
            append_line(&mut lines, line, &mut glyph_count, &mut run_count)?;
        }
        let height = lines.len() as f64 * options.line_height;
        if !height.is_finite() {
            return Err(TextError::InvalidInput);
        }
        Ok(Self {
            text: text.to_owned(),
            options,
            direction: text_options.direction,
            lines,
            glyph_count,
            height,
        })
    }
    /// Complete immutable original UTF8 source, including hard separators.
    pub fn text(&self) -> &str {
        &self.text
    }
    /// Validated physical alignment, width and baseline spacing.
    pub fn options(&self) -> ParagraphOptions {
        self.options
    }
    /// Explicit paragraph base direction used during layout.
    pub fn direction(&self) -> TextDirection {
        self.direction
    }
    pub fn lines(&self) -> &[ParagraphLine] {
        &self.lines
    }
    pub fn glyph_count(&self) -> usize {
        self.glyph_count
    }
    /// Baseline-spacing extent, not ink bounds or measured font ascender/descent.
    pub fn height(&self) -> f64 {
        self.height
    }
}
fn separator_start(text: &str, start: usize, end: usize) -> usize {
    let Some(c) = text[start..end].chars().next_back() else {
        return end;
    };
    if matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}') {
        let mut result = end - c.len_utf8();
        if c == '\n' && result > start && text.as_bytes()[result - 1] == b'\r' {
            result -= 1;
        }
        result
    } else {
        end
    }
}
fn shape_line(
    font: &TextFont,
    text: &str,
    bidi: &unicode_bidi::BidiInfo<'_>,
    range: Range<usize>,
    options: TextOptions<'_>,
) -> Result<ParagraphLine, TextError> {
    let mut line = ParagraphLine {
        separator: range.end..range.end,
        range: range.clone(),
        runs: Vec::new(),
        width: 0.,
        baseline: (0., 0.),
        overflows: false,
    };
    if range.is_empty() {
        return Ok(line);
    }
    let paragraph = bidi
        .paragraphs
        .iter()
        .find(|p| range.start >= p.range.start && range.end <= p.range.end)
        .ok_or(TextError::Shaping)?;
    let (levels, ranges) = bidi.visual_runs(paragraph, range);
    if ranges.len() > MAXIMUM_PARAGRAPH_RUNS {
        return Err(TextError::Capacity);
    }
    let mut glyph_count = 0;
    for range in ranges {
        if range.is_empty()
            || range.end > text.len()
            || !text.is_char_boundary(range.start)
            || !text.is_char_boundary(range.end)
        {
            return Err(TextError::Shaping);
        }
        let level = levels[range.start];
        let shape = font.shape(
            &text[range.clone()],
            TextOptions {
                direction: if level.is_rtl() {
                    TextDirection::RightToLeft
                } else {
                    TextDirection::LeftToRight
                },
                ..options
            },
        )?;
        glyph_count += shape.glyphs().len();
        if glyph_count > MAXIMUM_SHAPED_GLYPHS {
            return Err(TextError::Capacity);
        }
        let width = shape.advance().0;
        if !width.is_finite() || width < 0. || shape.advance().1 != 0. {
            return Err(TextError::Shaping);
        }
        line.runs.push(ParagraphRun {
            range,
            shape,
            x: line.width,
            level: level.number(),
        });
        line.width += width;
    }
    if !line.width.is_finite() {
        return Err(TextError::Shaping);
    }
    Ok(line)
}
fn place_line(
    line: &mut ParagraphLine,
    index: usize,
    options: ParagraphOptions,
) -> Result<(), TextError> {
    line.overflows = line.width > options.width;
    let room = (options.width - line.width).max(0.);
    line.baseline = (
        match options.alignment {
            TextAlignment::Left => 0.,
            TextAlignment::Center => room / 2.,
            TextAlignment::Right => room,
        },
        index as f64 * options.line_height,
    );
    if !line.baseline.1.is_finite() {
        return Err(TextError::InvalidInput);
    }
    Ok(())
}
fn append_line(
    lines: &mut Vec<ParagraphLine>,
    line: ParagraphLine,
    glyph_count: &mut usize,
    run_count: &mut usize,
) -> Result<(), TextError> {
    *glyph_count += line
        .runs
        .iter()
        .map(|r| r.shape.glyphs().len())
        .sum::<usize>();
    *run_count += line.runs.len();
    if lines.len() == MAXIMUM_PARAGRAPH_LINES
        || *run_count > MAXIMUM_PARAGRAPH_RUNS
        || *glyph_count > MAXIMUM_SHAPED_GLYPHS
    {
        return Err(TextError::Capacity);
    }
    lines.push(line);
    Ok(())
}
