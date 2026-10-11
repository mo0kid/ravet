//! Caller-owned font-stack wrapping with explicit scripts and mixed bidi.
//!
//! Preparation keeps original UTF8 source and exact selected faces. Unicode16 bidi,
//! Unicode15 line opportunities and Unicode17 graphemes are separate pinned profiles.
//! Allocating UI/worker work, never audio-callback or deadline support.
use crate::{
    fallback::{TextFontStack, choose_font, validate_coverage},
    paragraph::{MAXIMUM_PARAGRAPH_LINES, MAXIMUM_PARAGRAPH_RUNS, ParagraphOptions, TextAlignment},
    text::{
        MAXIMUM_SHAPED_GLYPHS, MAXIMUM_TEXT_BYTES, ShapedText, TextDirection, TextError,
        TextFeature, TextFont, TextOptions, valid_paragraph,
    },
};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

/// Maximum caller-authored contiguous script spans.
pub const MAXIMUM_PARAGRAPH_SCRIPTS: usize = 128;
/// Maximum coverage probes over the complete construction, including discarded lines.
pub const MAXIMUM_PARAGRAPH_PROBES: usize = 8192;
/// Maximum validation, probe and final contextual backend shape calls combined.
pub const MAXIMUM_PARAGRAPH_SHAPES: usize = 16384;
/// Maximum measured candidate lines, including empty output lines.
pub const MAXIMUM_PARAGRAPH_CANDIDATES: usize = 1024;

/// Explicit original-source script assignment. Boundaries must be complete graphemes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParagraphScriptSpan {
    pub range: Range<usize>,
    pub script: [u8; 4],
}
/// Caller-authored script/language/features; bidi resolves from an explicit base level.
#[derive(Clone, Copy, Debug)]
pub struct FallbackParagraphTextOptions<'a> {
    pub size: f64,
    pub base_direction: TextDirection,
    pub language: Option<&'a str>,
    pub features: &'a [TextFeature],
    pub scripts: &'a [ParagraphScriptSpan],
}
/// Exact admitted whole-construction work, not CPU time or backend memory.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParagraphWork {
    pub probes: usize,
    pub shapes: usize,
    pub candidates: usize,
}
impl ParagraphWork {
    fn shape(&mut self) -> Result<(), TextError> {
        if self.shapes == MAXIMUM_PARAGRAPH_SHAPES {
            return Err(TextError::Capacity);
        }
        self.shapes += 1;
        Ok(())
    }
    fn probe(&mut self) -> Result<(), TextError> {
        if self.probes == MAXIMUM_PARAGRAPH_PROBES {
            return Err(TextError::Capacity);
        }
        self.shape()?;
        self.probes += 1;
        Ok(())
    }
    fn candidate(&mut self) -> Result<(), TextError> {
        if self.candidates == MAXIMUM_PARAGRAPH_CANDIDATES {
            return Err(TextError::Capacity);
        }
        self.candidates += 1;
        Ok(())
    }
}
/// One visual homogeneous script/direction/face run, with original logical bytes.
pub struct FallbackParagraphRun<'f> {
    range: Range<usize>,
    context: Range<usize>,
    script: [u8; 4],
    level: u8,
    font_index: usize,
    font: &'f TextFont,
    shape: ShapedText,
    x: f64,
}
impl<'f> FallbackParagraphRun<'f> {
    pub fn range(&self) -> Range<usize> {
        self.range.clone()
    }
    /// Entire original hard-domain shaping context, excluding hard separator bytes.
    pub fn context_range(&self) -> Range<usize> {
        self.context.clone()
    }
    pub fn script(&self) -> [u8; 4] {
        self.script
    }
    pub fn level(&self) -> u8 {
        self.level
    }
    pub fn direction(&self) -> TextDirection {
        if self.level & 1 == 0 {
            TextDirection::LeftToRight
        } else {
            TextDirection::RightToLeft
        }
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
    /// Physical X offset from this line's aligned baseline.
    pub fn x(&self) -> f64 {
        self.x
    }
    pub fn source_cluster(&self, glyph_index: usize) -> Option<usize> {
        self.shape
            .glyphs()
            .get(glyph_index)
            .map(|g| self.range.start + g.cluster as usize)
    }
}
/// One logical line with visual runs and unchanged original separator bytes.
pub struct FallbackParagraphLine<'f> {
    range: Range<usize>,
    separator: Range<usize>,
    runs: Vec<FallbackParagraphRun<'f>>,
    missing: Vec<Range<usize>>,
    width: f64,
    baseline: (f64, f64),
    overflows: bool,
}
impl<'f> FallbackParagraphLine<'f> {
    pub fn range(&self) -> Range<usize> {
        self.range.clone()
    }
    pub fn separator(&self) -> Range<usize> {
        self.separator.clone()
    }
    pub fn runs(&self) -> &[FallbackParagraphRun<'f>] {
        &self.runs
    }
    pub fn width(&self) -> f64 {
        self.width
    }
    /// Physical alignment X and downward-Y baseline, first line at Y zero.
    pub fn baseline(&self) -> (f64, f64) {
        self.baseline
    }
    /// Retained advance (including trailing whitespace) exceeded width; no forced split or implicit clip.
    pub fn overflows(&self) -> bool {
        self.overflows
    }
}
/// Immutable complete wrapped output borrowing only the exact caller-selected faces.
/// The stack object can be dropped while those fonts remain alive.
///
/// ```compile_fail
/// use ravet_surface::{fallback::TextFontStack, fallback_paragraph::*, paragraph::ParagraphOptions, text::TextFont};
/// fn escape<'a>(bytes:&[u8], text:&str, style:FallbackParagraphTextOptions<'_>, geometry:ParagraphOptions)
/// -> FallbackParagraphLayout<'a> {
///     let font=TextFont::from_bytes(bytes,0).unwrap();
///     FallbackParagraphLayout::new(&TextFontStack::new(&[&font]).unwrap(),text,style,geometry).unwrap()
/// }
/// ```
pub struct FallbackParagraphLayout<'f> {
    text: String,
    scripts: Vec<ParagraphScriptSpan>,
    size: f64,
    direction: TextDirection,
    language: Option<String>,
    features: Vec<TextFeature>,
    options: ParagraphOptions,
    lines: Vec<FallbackParagraphLine<'f>>,
    missing: Vec<Range<usize>>,
    glyphs: usize,
    missing_glyphs: usize,
    height: f64,
    work: ParagraphWork,
}
impl<'f> FallbackParagraphLayout<'f> {
    /// Prepare <=4096 original UTF8 bytes and <=128 explicit contiguous script spans.
    /// Resolve full-source Unicode16 bidi, wrap at Unicode15 opportunities that are
    /// Unicode17 grapheme boundaries, then select only supplied fonts for whole units.
    ///
    /// Context is the full original content between LF/CRLF/CR/U+2028/U+2029 hard
    /// separators. Soft wraps/font/script/bidi boundaries do not reset BOT/EOT;
    /// U+2028 does not reset full-source bidi embedding. Default ignorables retain
    /// source/context and disappear after shaping. Missing units use primary with
    /// explicit ranges; unexpected final covered glyph zero rejects everything.
    ///
    /// Greedy first-overflow fitting preserves whitespace; oversized first units
    /// overflow unchanged. Output<=256 lines/512 visual runs/16384 glyphs. Shared
    /// Work is width-dependent and not monotonic; wider candidates may exhaust a budget
    /// even when narrower geometry fits. Whole-call budgets<=8192 probes/16384 shapes/1024 candidates include discarded
    /// lines. Backend transient memory/time remains external; no partial publication.
    ///
    /// ```
    /// use ravet_surface::{fallback::TextFontStack, fallback_paragraph::*,
    ///     paragraph::{ParagraphOptions,TextAlignment},text::{TextFont,TextDirection}};
    /// let primary=TextFont::from_bytes(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"),
    ///     "/tests/fixtures/fallback/RavetFallbackPrimary.ttf")),0)?;
    /// let alternative=TextFont::from_bytes(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"),
    ///     "/tests/fixtures/fallback/RavetFallbackComplement.ttf")),0)?;
    /// let stack=TextFontStack::new(&[&primary,&alternative])?;
    /// let scripts=[ParagraphScriptSpan {range:0..8,script:*b"Latn"}];
    /// let layout=FallbackParagraphLayout::new(&stack,"ab ab ab",
    ///     FallbackParagraphTextOptions {size:10.,base_direction:TextDirection::LeftToRight,
    ///         language:None,features:&[],scripts:&scripts},
    ///     ParagraphOptions {width:14.5,line_height:15.,alignment:TextAlignment::Left})?;
    /// assert_eq!(layout.lines().len(),3);
    /// assert_eq!(layout.lines()[1].range(),3..6);
    /// assert!(std::ptr::eq(layout.lines()[0].runs()[0].font(),&primary));
    /// # Ok::<(),ravet_surface::text::TextError>(())
    /// ```
    pub fn new(
        stack: &TextFontStack<'f>,
        text: &str,
        style: FallbackParagraphTextOptions<'_>,
        options: ParagraphOptions,
    ) -> Result<Self, TextError> {
        if text.len() > MAXIMUM_TEXT_BYTES || style.scripts.len() > MAXIMUM_PARAGRAPH_SCRIPTS {
            return Err(TextError::Capacity);
        }
        if !valid_paragraph(text)
            || !options.width.is_finite()
            || options.width <= 0.
            || !options.line_height.is_finite()
            || options.line_height <= 0.
        {
            return Err(TextError::InvalidInput);
        }
        let mut boundaries: Vec<_> = text.grapheme_indices(true).map(|(i, _)| i).collect();
        boundaries.push(text.len());
        let mut end = 0;
        for span in style.scripts {
            if span.range.start != end
                || span.range.start >= span.range.end
                || span.range.end > text.len()
                || span.script.iter().any(|b| !b.is_ascii_alphabetic())
                || boundaries.binary_search(&span.range.start).is_err()
                || boundaries.binary_search(&span.range.end).is_err()
            {
                return Err(TextError::InvalidInput);
            }
            end = span.range.end;
        }
        if end != text.len() {
            return Err(TextError::InvalidInput);
        }
        let domains = hard_domains(text)?;
        let base = match style.base_direction {
            TextDirection::LeftToRight => unicode_bidi::Level::ltr(),
            TextDirection::RightToLeft => unicode_bidi::Level::rtl(),
        };
        let mut engine = Engine {
            fonts: stack.fonts(),
            text,
            style,
            boundaries,
            choices: vec![[None; 2]; text.graphemes(true).count()],
            work: ParagraphWork::default(),
        };
        // Empty sources still validate global style; nonempty validates every tag.
        if style.scripts.is_empty() {
            engine.work.shape()?;
            engine.fonts[0].shape("", engine.options(*b"Zyyy", style.base_direction))?;
        } else {
            for span in style.scripts {
                engine.work.shape()?;
                engine.fonts[0].shape("", engine.options(span.script, style.base_direction))?;
            }
        }
        let bidi = unicode_bidi::BidiInfo::new(text, Some(base));
        let mut lines = Vec::new();
        let mut glyphs = 0;
        let mut run_count = 0;
        let mut missing_glyphs = 0;
        let mut missing = Vec::new();
        for domain in domains {
            let mut start = domain.content.start;
            let breaks: Vec<_> = unicode_linebreak::linebreaks(&text[domain.content.clone()])
                .map(|(i, _)| domain.content.start + i)
                .filter(|i| engine.boundaries.binary_search(i).is_ok())
                .collect();
            let mut cursor = 0;
            loop {
                let mut selected = None;
                if domain.content.is_empty() {
                    selected = Some((0, engine.line(&bidi, &domain.content, start..start)?));
                } else {
                    for (index, &end) in breaks.iter().enumerate().skip(cursor) {
                        let candidate = engine.line(&bidi, &domain.content, start..end)?;
                        let overflow = candidate.width > options.width;
                        if !overflow || selected.is_none() {
                            selected = Some((index, candidate));
                        }
                        if overflow || end == domain.content.end {
                            break;
                        }
                    }
                }
                let (index, mut line) = selected.ok_or(TextError::Shaping)?;
                let last = line.range.end == domain.content.end;
                if last {
                    line.separator = domain.separator.clone();
                }
                line.overflows = line.width > options.width;
                let room = (options.width - line.width).max(0.);
                line.baseline = (
                    match options.alignment {
                        TextAlignment::Left => 0.,
                        TextAlignment::Center => room / 2.,
                        TextAlignment::Right => room,
                    },
                    lines.len() as f64 * options.line_height,
                );
                if !line.baseline.0.is_finite() || !line.baseline.1.is_finite() {
                    return Err(TextError::InvalidInput);
                }
                let count: usize = line.runs.iter().map(|r| r.shape.glyphs().len()).sum();
                if lines.len() == MAXIMUM_PARAGRAPH_LINES
                    || line.runs.len() > MAXIMUM_PARAGRAPH_RUNS - run_count
                    || count > MAXIMUM_SHAPED_GLYPHS - glyphs
                {
                    return Err(TextError::Capacity);
                }
                run_count += line.runs.len();
                glyphs += count;
                missing_glyphs += line
                    .runs
                    .iter()
                    .map(|r| r.shape.missing_glyphs())
                    .sum::<usize>();
                missing.extend(line.missing.iter().cloned());
                start = line.range.end;
                lines.push(line);
                if last {
                    break;
                }
                cursor = index + 1;
            }
        }
        missing.sort_by_key(|r| r.start);
        let height = lines.len() as f64 * options.line_height;
        if !height.is_finite() {
            return Err(TextError::InvalidInput);
        }
        Ok(Self {
            text: text.to_owned(),
            scripts: style.scripts.to_vec(),
            size: style.size,
            direction: style.base_direction,
            language: style.language.map(str::to_owned),
            features: style.features.to_vec(),
            options,
            lines,
            missing,
            glyphs,
            missing_glyphs,
            height,
            work: engine.work,
        })
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn scripts(&self) -> &[ParagraphScriptSpan] {
        &self.scripts
    }
    pub fn size(&self) -> f64 {
        self.size
    }
    pub fn direction(&self) -> TextDirection {
        self.direction
    }
    pub fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }
    pub fn features(&self) -> &[TextFeature] {
        &self.features
    }
    pub fn options(&self) -> ParagraphOptions {
        self.options
    }
    pub fn lines(&self) -> &[FallbackParagraphLine<'f>] {
        &self.lines
    }
    pub fn missing_ranges(&self) -> &[Range<usize>] {
        &self.missing
    }
    pub fn glyph_count(&self) -> usize {
        self.glyphs
    }
    pub fn missing_glyphs(&self) -> usize {
        self.missing_glyphs
    }
    pub fn height(&self) -> f64 {
        self.height
    }
    pub fn work(&self) -> ParagraphWork {
        self.work
    }
}
#[derive(Clone, Copy)]
struct Choice {
    index: usize,
    missing: bool,
}
struct Engine<'a, 'f> {
    fonts: &'a [&'f TextFont],
    text: &'a str,
    style: FallbackParagraphTextOptions<'a>,
    boundaries: Vec<usize>,
    choices: Vec<[Option<Choice>; 2]>,
    work: ParagraphWork,
}
impl<'f> Engine<'_, 'f> {
    fn options(&self, script: [u8; 4], direction: TextDirection) -> TextOptions<'_> {
        TextOptions {
            size: self.style.size,
            direction,
            script: Some(script),
            language: self.style.language,
            features: self.style.features,
        }
    }
    fn line(
        &mut self,
        bidi: &unicode_bidi::BidiInfo<'_>,
        domain: &Range<usize>,
        range: Range<usize>,
    ) -> Result<FallbackParagraphLine<'f>, TextError> {
        self.work.candidate()?;
        let mut line = FallbackParagraphLine {
            separator: range.end..range.end,
            range: range.clone(),
            runs: Vec::new(),
            missing: Vec::new(),
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
        // Apply L1 on original scalar data, then L2 to whole grapheme units.
        // X9-removed controls (including BN joiners/tags) carry the owning
        // grapheme's retained scalar level; they cannot split that grapheme
        // when L1 resets a trailing formatting sequence.
        let scalar_levels = bidi.reordered_levels(paragraph, range.clone());
        let first = self
            .boundaries
            .binary_search(&range.start)
            .map_err(|_| TextError::Shaping)?;
        let last = self
            .boundaries
            .binary_search(&range.end)
            .map_err(|_| TextError::Shaping)?;
        let mut logical: Vec<Range<usize>> = Vec::new();
        let mut levels = Vec::new();
        for unit in first..last {
            let grapheme = self.boundaries[unit]..self.boundaries[unit + 1];
            let retained = self.text[grapheme.clone()]
                .char_indices()
                .filter_map(|(offset, _)| {
                    let index = grapheme.start + offset;
                    (!matches!(
                        bidi.original_classes[index],
                        unicode_bidi::BidiClass::BN
                            | unicode_bidi::BidiClass::RLE
                            | unicode_bidi::BidiClass::LRE
                            | unicode_bidi::BidiClass::RLO
                            | unicode_bidi::BidiClass::LRO
                            | unicode_bidi::BidiClass::PDF
                    ))
                    .then_some(scalar_levels[index])
                })
                .collect::<Vec<_>>();
            let level = retained
                .last()
                .copied()
                .unwrap_or(scalar_levels[grapheme.start]);
            // RAVET's atomic-grapheme rendering refinement uses the final
            // retained scalar's L1 level (including an attached base/mark after
            // Prepend). UAX #9 scalar resolution itself remains unchanged;
            // this refinement is not scalar-exact UAX #9 L2 conformance.
            if levels.last() == Some(&level) {
                logical.last_mut().expect("matching preceding level").end = grapheme.end;
            } else {
                logical.push(grapheme);
                levels.push(level);
            }
        }
        let visual = unicode_bidi::BidiInfo::reorder_visual(&levels);
        let mut glyphs = 0;
        for index in visual {
            let range = logical[index].clone();
            if range.is_empty()
                || self.boundaries.binary_search(&range.start).is_err()
                || self.boundaries.binary_search(&range.end).is_err()
            {
                return Err(TextError::Shaping);
            }
            let level = levels[index];
            let direction = if level.is_rtl() {
                TextDirection::RightToLeft
            } else {
                TextDirection::LeftToRight
            };
            let mut pieces: Vec<(Range<usize>, [u8; 4])> = Vec::new();
            for span in self.style.scripts {
                let piece = span.range.start.max(range.start)..span.range.end.min(range.end);
                if piece.is_empty() {
                    continue;
                }
                if let Some((previous, script)) = pieces.last_mut()
                    && *script == span.script
                    && previous.end == piece.start
                {
                    previous.end = piece.end;
                } else {
                    pieces.push((piece, span.script));
                }
            }
            if level.is_rtl() {
                pieces.reverse();
            }
            for (range, script) in pieces {
                let first = self
                    .boundaries
                    .binary_search(&range.start)
                    .map_err(|_| TextError::Shaping)?;
                let last = self
                    .boundaries
                    .binary_search(&range.end)
                    .map_err(|_| TextError::Shaping)?;
                let mut selected: Vec<(Range<usize>, usize)> = Vec::new();
                let mut missing = Vec::new();
                for unit in first..last {
                    let absolute = self.boundaries[unit]..self.boundaries[unit + 1];
                    let slot = usize::from(level.is_rtl());
                    let chosen = if let Some(chosen) = self.choices[unit][slot] {
                        chosen
                    } else {
                        let local = absolute.start - domain.start..absolute.end - domain.start;
                        // Copy options before borrowing the shared work counter mutably.
                        let options = TextOptions {
                            size: self.style.size,
                            direction,
                            script: Some(script),
                            language: self.style.language,
                            features: self.style.features,
                        };
                        let work = &mut self.work;
                        let found = choose_font(
                            self.fonts,
                            &self.text[domain.clone()],
                            local,
                            options,
                            || work.probe(),
                        )?;
                        let chosen = Choice {
                            index: found.unwrap_or(0),
                            missing: found.is_none(),
                        };
                        self.choices[unit][slot] = Some(chosen);
                        chosen
                    };
                    if chosen.missing {
                        missing.push(absolute.clone());
                    }
                    if let Some((previous, index)) = selected.last_mut()
                        && *index == chosen.index
                    {
                        previous.end = absolute.end;
                    } else {
                        if selected.len() == MAXIMUM_PARAGRAPH_RUNS {
                            return Err(TextError::Capacity);
                        }
                        selected.push((absolute, chosen.index));
                    }
                }
                if level.is_rtl() {
                    selected.reverse();
                }
                line.missing.extend(missing.iter().cloned());
                for (absolute, index) in selected {
                    if line.runs.len() == MAXIMUM_PARAGRAPH_RUNS {
                        return Err(TextError::Capacity);
                    }
                    self.work.shape()?;
                    let shape = self.fonts[index].shape_contextual(
                        &self.text[domain.clone()],
                        absolute.start - domain.start..absolute.end - domain.start,
                        self.options(script, direction),
                        true,
                    )?;
                    validate_coverage(self.text, &absolute, &shape, &missing)?;
                    if shape.glyphs().len() > MAXIMUM_SHAPED_GLYPHS - glyphs {
                        return Err(TextError::Capacity);
                    }
                    glyphs += shape.glyphs().len();
                    let advance = shape.advance();
                    if !advance.0.is_finite() || advance.0 < 0. || advance.1 != 0. {
                        return Err(TextError::Shaping);
                    }
                    let x = line.width;
                    line.width += advance.0;
                    if !line.width.is_finite() {
                        return Err(TextError::Shaping);
                    }
                    line.runs.push(FallbackParagraphRun {
                        range: absolute,
                        context: domain.clone(),
                        script,
                        level: level.number(),
                        font_index: index,
                        font: self.fonts[index],
                        shape,
                        x,
                    });
                }
            }
        }
        Ok(line)
    }
}
struct Domain {
    content: Range<usize>,
    separator: Range<usize>,
}
fn hard_domains(text: &str) -> Result<Vec<Domain>, TextError> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((index, c)) = chars.next() {
        if matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}') {
            let mut end = index + c.len_utf8();
            if c == '\r' && chars.peek().is_some_and(|(_, c)| *c == '\n') {
                end = chars.next().expect("peeked LF").0 + 1;
            }
            if result.len() == MAXIMUM_PARAGRAPH_LINES {
                return Err(TextError::Capacity);
            }
            result.push(Domain {
                content: start..index,
                separator: index..end,
            });
            start = end;
        }
    }
    if result.len() == MAXIMUM_PARAGRAPH_LINES {
        return Err(TextError::Capacity);
    }
    result.push(Domain {
        content: start..text.len(),
        separator: text.len()..text.len(),
    });
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn whole_counter_failures_keep_the_exact_last_admitted_values() {
        let mut work = ParagraphWork {
            probes: MAXIMUM_PARAGRAPH_PROBES - 1,
            shapes: MAXIMUM_PARAGRAPH_SHAPES - 1,
            candidates: MAXIMUM_PARAGRAPH_CANDIDATES - 1,
        };
        assert_eq!(work.probe(), Ok(()));
        assert_eq!(work.candidate(), Ok(()));
        let admitted = work;
        assert_eq!(work.probe(), Err(TextError::Capacity));
        assert_eq!(work.shape(), Err(TextError::Capacity));
        assert_eq!(work.candidate(), Err(TextError::Capacity));
        assert_eq!(work, admitted);
        // Probe capacity remains unused when the shared total backend-call budget refuses.
        let mut work = ParagraphWork {
            shapes: MAXIMUM_PARAGRAPH_SHAPES,
            ..Default::default()
        };
        assert_eq!(work.probe(), Err(TextError::Capacity));
        assert_eq!(work.probes, 0);
    }
}
