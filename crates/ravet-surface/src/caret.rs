//! Prepared paragraph source positions, hit tests and selection pen rectangles.
//! Extended grapheme boundaries use pinned Unicode 17.0 data. Ligature interiors
//! use equal-grapheme pen interpolation, not font GDEF caret measurements.
use crate::{
    layout::Rect,
    paragraph::ParagraphLayout,
    text::{MAXIMUM_TEXT_BYTES, TextDirection, TextError},
};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

/// At a wrap/bidi boundary, choose the end of preceding logical text or beginning
/// of following logical text. These may have different visual positions/lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaretAffinity {
    /// End of preceding logical text.
    Upstream,
    /// Start of following logical text.
    Downstream,
}
/// Insertion position at an extended grapheme boundary, in original UTF8 bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextPosition {
    /// Original source UTF8 byte offset, at an extended grapheme boundary.
    pub byte_offset: usize,
    /// Side to preserve at a soft-wrap or directional boundary.
    pub affinity: CaretAffinity,
}
/// Paragraph-relative pen position and the selected line. Y is its baseline;
/// UI owners supply font ascender/ink styling and any document/window transform.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Caret {
    /// Source insertion position and retained affinity.
    pub position: TextPosition,
    /// Zero-based line index.
    pub line: usize,
    /// Paragraph-relative X and baseline Y.
    pub point: (f64, f64),
}
/// Selection pen rectangle and original line index, in paragraph coordinates.
/// Boxes use baseline +/- half the configured line height, rather than font ink.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectionRect {
    /// Zero-based line index.
    pub line: usize,
    /// Visual pen box, independent of glyph ink.
    pub rectangle: Rect,
}
impl SelectionRect {
    /// Zero-sized placeholder for reusable caller-owned output buffers.
    pub fn empty() -> Self {
        Self {
            line: 0,
            rectangle: Rect::new(0., 0., 0., 0.).unwrap(),
        }
    }
}
/// At most two affinities per input-byte boundary.
pub const MAXIMUM_CARET_POSITIONS: usize = 2 * (MAXIMUM_TEXT_BYTES + 1);
/// At most one selection cell per extended grapheme, bounded by input bytes.
pub const MAXIMUM_SELECTION_RECTS: usize = MAXIMUM_TEXT_BYTES;
#[derive(Clone, Copy)]
struct Pair {
    upstream: Caret,
    downstream: Caret,
}
#[derive(Clone)]
struct Cell {
    source: Range<usize>,
    selection: SelectionRect,
}
#[derive(Clone, Copy)]
struct Group {
    byte: usize,
    left: f64,
    right: f64,
}
/// Prepared immutable map borrowing its complete source/layout identity.
/// Preparation allocates; lookup/hit testing and caller-buffer selections do not.
/// Unsupported non-monotone or cross-grapheme backend clusters reject instead of
/// inventing editable byte positions. No interactive input or font fallback.
pub struct CaretMap<'a> {
    layout: &'a ParagraphLayout,
    boundaries: Vec<usize>,
    positions: Vec<Pair>,
    cells: Vec<Cell>,
}
impl<'a> CaretMap<'a> {
    /// Prepare at most 8194 affinity positions and 4096 grapheme cells from an
    /// immutable layout retaining its complete source. Reject backend clusters
    /// that are non-monotone, cross graphemes or omit an initial source boundary.
    /// Ligatures use equal-grapheme advance interpolation, without GDEF queries.
    /// This allocates and must run outside audio callbacks.
    pub fn new(layout: &'a ParagraphLayout) -> Result<Self, TextError> {
        let text = layout.text();
        let boundaries: Vec<_> = text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .chain(std::iter::once(text.len()))
            .collect();
        let mut positions = vec![[None; 2]; boundaries.len()];
        let mut cells = Vec::new();
        let line_height = layout.options().line_height;
        for (line_index, line) in layout.lines().iter().enumerate() {
            for run in line.runs() {
                let range = run.range();
                let mut groups = Vec::<Group>::new();
                let mut pen = 0.;
                for glyph in run.shape().glyphs() {
                    let byte = range.start + glyph.cluster as usize;
                    if byte >= range.end || boundaries.binary_search(&byte).is_err() {
                        return Err(TextError::Shaping);
                    }
                    if groups.last().is_none_or(|g| g.byte != byte) {
                        groups.push(Group {
                            byte,
                            left: pen,
                            right: pen,
                        });
                    }
                    pen += glyph.x_advance;
                    groups.last_mut().unwrap().right = pen;
                }
                if groups.is_empty() {
                    groups.push(Group {
                        byte: range.start,
                        left: 0.,
                        right: 0.,
                    });
                }
                let mut logical: Vec<_> = groups.iter().map(|g| g.byte).collect();
                logical.sort_unstable();
                logical.dedup();
                if logical.len() != groups.len() || logical[0] != range.start {
                    return Err(TextError::Shaping);
                }
                let direction = run.shape().direction();
                if groups.windows(2).any(|g| match direction {
                    TextDirection::LeftToRight => g[0].byte >= g[1].byte,
                    TextDirection::RightToLeft => g[0].byte <= g[1].byte,
                }) {
                    return Err(TextError::Shaping);
                }
                for group in groups {
                    let logical_index = logical.binary_search(&group.byte).unwrap();
                    let end = logical.get(logical_index + 1).copied().unwrap_or(range.end);
                    let first = boundaries
                        .binary_search(&group.byte)
                        .map_err(|_| TextError::Shaping)?;
                    let last = boundaries
                        .binary_search(&end)
                        .map_err(|_| TextError::Shaping)?;
                    if last <= first || group.right < group.left || !group.right.is_finite() {
                        return Err(TextError::Shaping);
                    }
                    let width = group.right - group.left;
                    let position = |index: usize| {
                        let fraction = (index - first) as f64 / (last - first) as f64;
                        let fraction = match direction {
                            TextDirection::LeftToRight => fraction,
                            TextDirection::RightToLeft => 1. - fraction,
                        };
                        let pen = if fraction == 0. {
                            group.left
                        } else if fraction == 1. {
                            group.right
                        } else {
                            group.left + width * fraction
                        };
                        line.baseline().0 + (run.x() + pen)
                    };
                    for index in first..last {
                        let x0 = position(index);
                        let x1 = position(index + 1);
                        let rectangle = Rect::new(
                            x0.min(x1),
                            line.baseline().1 - line_height / 2.,
                            (x1 - x0).abs(),
                            line_height,
                        )
                        .ok_or(TextError::Shaping)?;
                        cells.push(Cell {
                            source: boundaries[index]..boundaries[index + 1],
                            selection: SelectionRect {
                                line: line_index,
                                rectangle,
                            },
                        });
                        record(
                            &mut positions,
                            index,
                            CaretAffinity::Downstream,
                            line_index,
                            (x0, line.baseline().1),
                        );
                        record(
                            &mut positions,
                            index + 1,
                            CaretAffinity::Upstream,
                            line_index,
                            (x1, line.baseline().1),
                        );
                    }
                }
            }
            let range = line.range();
            let first = boundaries
                .binary_search(&range.start)
                .map_err(|_| TextError::Shaping)?;
            let last = boundaries
                .binary_search(&range.end)
                .map_err(|_| TextError::Shaping)?;
            let (start_x, end_x) = match layout.direction() {
                TextDirection::LeftToRight => (line.baseline().0, line.baseline().0 + line.width()),
                TextDirection::RightToLeft => (line.baseline().0 + line.width(), line.baseline().0),
            };
            if positions[first][1].is_none() {
                record(
                    &mut positions,
                    first,
                    CaretAffinity::Downstream,
                    line_index,
                    (start_x, line.baseline().1),
                );
            }
            if positions[last][0].is_none() {
                record(
                    &mut positions,
                    last,
                    CaretAffinity::Upstream,
                    line_index,
                    (end_x, line.baseline().1),
                );
            }
        }
        let positions = positions
            .into_iter()
            .enumerate()
            .map(|(index, pair)| {
                let upstream = pair[0].or(pair[1]).ok_or(TextError::Shaping)?;
                let downstream = pair[1].or(pair[0]).ok_or(TextError::Shaping)?;
                Ok(Pair {
                    upstream: Caret {
                        position: TextPosition {
                            byte_offset: boundaries[index],
                            affinity: CaretAffinity::Upstream,
                        },
                        ..upstream
                    },
                    downstream: Caret {
                        position: TextPosition {
                            byte_offset: boundaries[index],
                            affinity: CaretAffinity::Downstream,
                        },
                        ..downstream
                    },
                })
            })
            .collect::<Result<Vec<_>, TextError>>()?;
        if cells.len() > MAXIMUM_SELECTION_RECTS || positions.len() * 2 > MAXIMUM_CARET_POSITIONS {
            return Err(TextError::Capacity);
        }
        cells.sort_by(|a, b| {
            a.selection.line.cmp(&b.selection.line).then_with(|| {
                a.selection
                    .rectangle
                    .x()
                    .total_cmp(&b.selection.rectangle.x())
            })
        });
        Ok(Self {
            layout,
            boundaries,
            positions,
            cells,
        })
    }
    /// Sorted original UTF8 extended-grapheme boundaries, including source end.
    pub fn boundaries(&self) -> &[usize] {
        &self.boundaries
    }
    /// Resolve a valid source boundary and affinity, or return `None`.
    pub fn caret(&self, position: TextPosition) -> Option<Caret> {
        let pair = self
            .positions
            .get(self.boundaries.binary_search(&position.byte_offset).ok()?)?;
        Some(match position.affinity {
            CaretAffinity::Upstream => pair.upstream,
            CaretAffinity::Downstream => pair.downstream,
        })
    }
    /// Nearest baseline and pen caret; outside coordinates clamp to the available
    /// lines/carets; a midpoint between baselines chooses the later line. Nonfinite
    /// coordinates return `None`. X ties prefer the requested affinity, then
    /// earlier source bytes. Invisible/bidi positions can share coordinates, so
    /// source-to-point-to-source round trips are not guaranteed.
    pub fn hit_test(&self, x: f64, y: f64, preferred: CaretAffinity) -> Option<Caret> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let last = self.layout.lines().len() - 1;
        let line = if y <= 0. {
            0
        } else if y >= self.layout.lines()[last].baseline().1 {
            last
        } else {
            ((y / self.layout.options().line_height) + 0.5).floor() as usize
        };
        let mut left = f64::INFINITY;
        let mut right = f64::NEG_INFINITY;
        for pair in &self.positions {
            for caret in [pair.upstream, pair.downstream] {
                if caret.line == line {
                    left = left.min(caret.point.0);
                    right = right.max(caret.point.0);
                }
            }
        }
        let x = x.clamp(left, right);
        let mut best = None::<(f64, Caret)>;
        for pair in &self.positions {
            for caret in [pair.upstream, pair.downstream] {
                if caret.line != line {
                    continue;
                }
                let distance = (x - caret.point.0).abs();
                if best.is_none_or(|(prior, value)| {
                    distance < prior
                        || (distance == prior
                            && caret.position.affinity == preferred
                            && value.position.affinity != preferred)
                }) {
                    best = Some((distance, caret));
                }
            }
        }
        best.map(|(_, caret)| caret)
    }
    /// Complete half-open logical selection at grapheme boundaries. Writes visual
    /// pen boxes in line/X order, merging touching boxes with a relative four-epsilon
    /// tolerance for accumulated floating-point edges. Hard separators have no
    /// pen width. Invalid input/short destinations preserve every destination slot.
    pub fn selection(
        &self,
        range: Range<usize>,
        destination: &mut [SelectionRect],
    ) -> Result<usize, TextError> {
        if range.start > range.end
            || self.boundaries.binary_search(&range.start).is_err()
            || self.boundaries.binary_search(&range.end).is_err()
        {
            return Err(TextError::InvalidInput);
        }
        let mut count = 0;
        self.visit_selection(&range, |_| count += 1);
        if count > destination.len() {
            return Err(TextError::Capacity);
        }
        let mut index = 0;
        self.visit_selection(&range, |selection| {
            destination[index] = selection;
            index += 1
        });
        Ok(count)
    }
    fn visit_selection(&self, range: &Range<usize>, mut emit: impl FnMut(SelectionRect)) {
        let mut pending = None::<SelectionRect>;
        for cell in &self.cells {
            if cell.source.start < range.start || cell.source.end > range.end {
                continue;
            }
            let next = cell.selection;
            if let Some(prior) = pending {
                let prior_right = prior.rectangle.x() + prior.rectangle.width();
                let tolerance =
                    prior_right.abs().max(next.rectangle.x().abs()) * (4. * f64::EPSILON);
                if prior.line == next.line && next.rectangle.x() - prior_right <= tolerance {
                    let right = (prior.rectangle.x() + prior.rectangle.width())
                        .max(next.rectangle.x() + next.rectangle.width());
                    pending = Some(SelectionRect {
                        rectangle: Rect::new(
                            prior.rectangle.x(),
                            prior.rectangle.y(),
                            right - prior.rectangle.x(),
                            prior.rectangle.height(),
                        )
                        .unwrap(),
                        ..prior
                    });
                    continue;
                }
                emit(prior);
            }
            pending = Some(next);
        }
        if let Some(prior) = pending {
            emit(prior)
        }
    }
}
fn record(
    positions: &mut [[Option<Caret>; 2]],
    index: usize,
    affinity: CaretAffinity,
    line: usize,
    point: (f64, f64),
) {
    let target = match affinity {
        CaretAffinity::Upstream => 0,
        CaretAffinity::Downstream => 1,
    };
    positions[index][target] = Some(Caret {
        position: TextPosition {
            byte_offset: 0,
            affinity,
        },
        line,
        point,
    });
}
