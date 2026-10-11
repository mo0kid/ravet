//! Deterministic bounded axis and explicit-grid UI layout without a native window dependency.

/// A finite rectangle with nonnegative extent and finite right/bottom edges.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}
impl Rect {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Option<Self> {
        if ![x, y, width, height, x + width, y + height]
            .iter()
            .all(|v| v.is_finite())
            || width < 0.
            || height < 0.
        {
            return None;
        }
        Some(Self {
            x,
            y,
            width,
            height,
        })
    }
    pub fn x(self) -> f64 {
        self.x
    }
    pub fn y(self) -> f64 {
        self.y
    }
    pub fn width(self) -> f64 {
        self.width
    }
    pub fn height(self) -> f64 {
        self.height
    }
    /// Half-open containment; empty rectangles contain no points.
    pub fn contains(self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
    /// Uniform padding; rejects negative/nonfinite padding or an inverted result.
    pub fn inset(self, padding: f64) -> Option<Self> {
        if !padding.is_finite() || padding < 0. {
            return None;
        }
        Self::new(
            self.x + padding,
            self.y + padding,
            self.width - 2. * padding,
            self.height - 2. * padding,
        )
    }
    /// Rectangle intersection; disjoint rectangles return an empty rectangle.
    pub fn intersection(self, other: Self) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        Self {
            x,
            y,
            width: ((self.x + self.width).min(other.x + other.width) - x).max(0.),
            height: ((self.y + self.height).min(other.y + other.height) - y).max(0.),
        }
    }
}
/// Row or column placement. Children stretch across the container's other axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}
/// A child's main-axis minimum, preferred extent, maximum and growth weight.
/// All values must be finite/nonnegative, with minimum <= preferred <= maximum.
#[derive(Clone, Copy, Debug)]
pub struct LayoutItem {
    pub minimum: f64,
    pub preferred: f64,
    pub maximum: f64,
    pub grow: f64,
}
impl LayoutItem {
    /// An extent that neither shrinks nor grows.
    pub fn fixed(extent: f64) -> Self {
        Self {
            minimum: extent,
            preferred: extent,
            maximum: extent,
            grow: 0.,
        }
    }
    fn valid(self) -> bool {
        [self.minimum, self.preferred, self.maximum, self.grow]
            .iter()
            .all(|v| v.is_finite() && *v >= 0.)
            && self.minimum <= self.preferred
            && self.preferred <= self.maximum
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutError {
    InvalidInput,
    Capacity,
    InsufficientSpace,
    Arithmetic,
}
/// Lay out at most 256 children into an equal-length caller-owned destination.
/// Gap is finite/nonnegative. Shrink distributes proportionally to each child's
/// preferred-minus-minimum capacity. Growth distributes by weight, redistributing
/// after maximum caps. Unclaimed space remains at the end. No wrapping, rounding,
/// implicit clipping, pixel scaling or allocation occurs. Every failure preserves
/// the entire destination; nested layouts are explicit calls by the consumer.
pub fn layout_axis(
    bounds: Rect,
    axis: Axis,
    gap: f64,
    items: &[LayoutItem],
    output: &mut [Rect],
) -> Result<(), LayoutError> {
    if items.len() > 256 || output.len() != items.len() {
        return Err(LayoutError::Capacity);
    }
    if !gap.is_finite() || gap < 0. || items.iter().any(|item| !item.valid()) {
        return Err(LayoutError::InvalidInput);
    }
    if items.is_empty() {
        return Ok(());
    }
    let extent = match axis {
        Axis::Horizontal => bounds.width,
        Axis::Vertical => bounds.height,
    };
    let available = extent - gap * (items.len() - 1) as f64;
    let minimum: f64 = items.iter().map(|i| i.minimum).sum();
    let preferred: f64 = items.iter().map(|i| i.preferred).sum();
    if !available.is_finite() || !minimum.is_finite() || !preferred.is_finite() {
        return Err(LayoutError::Arithmetic);
    }
    if available < minimum {
        return Err(LayoutError::InsufficientSpace);
    }
    let mut sizes = [0.; 256];
    if preferred > available {
        let ratio = (available - minimum) / (preferred - minimum);
        for (size, item) in sizes.iter_mut().zip(items) {
            *size = item.minimum + (item.preferred - item.minimum) * ratio;
        }
    } else {
        for (size, item) in sizes.iter_mut().zip(items) {
            *size = item.preferred;
        }
        let mut remaining = available - preferred;
        for _ in 0..=items.len() {
            if remaining <= 0. {
                break;
            }
            // Normalize weights before summing to avoid overflow for large weights.
            let scale = items
                .iter()
                .zip(&sizes)
                .filter(|(item, size)| item.grow > 0. && **size < item.maximum)
                .map(|(item, _)| item.grow)
                .fold(0., f64::max);
            if scale == 0. {
                break;
            }
            let total: f64 = items
                .iter()
                .zip(&sizes)
                .filter(|(item, size)| **size < item.maximum)
                .map(|(item, _)| item.grow / scale)
                .sum();
            let mut used = 0.;
            let mut capped = false;
            for (item, size) in items.iter().zip(&mut sizes) {
                if item.grow == 0. || *size >= item.maximum {
                    continue;
                }
                let share = remaining * (item.grow / scale / total);
                let added = share.min(item.maximum - *size);
                capped |= share >= item.maximum - *size;
                *size += added;
                used += added;
            }
            remaining = (remaining - used).max(0.);
            if !capped || used == 0. {
                break;
            }
        }
    }
    let mut result = [bounds; 256];
    let mut position = match axis {
        Axis::Horizontal => bounds.x,
        Axis::Vertical => bounds.y,
    };
    for (index, size) in sizes[..items.len()].iter().enumerate() {
        result[index] = match axis {
            Axis::Horizontal => Rect::new(position, bounds.y, *size, bounds.height),
            Axis::Vertical => Rect::new(bounds.x, position, bounds.width, *size),
        }
        .ok_or(LayoutError::Arithmetic)?;
        position += size + gap;
    }
    output.copy_from_slice(&result[..items.len()]);
    Ok(())
}

/// Explicit placement into a bounded grid's zero-based row and column tracks.
/// Spans are nonzero and must fit entirely inside their corresponding tracks.
/// Areas may overlap; their output order is the caller's order, not paint order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridArea {
    pub row: usize,
    pub column: usize,
    pub row_span: usize,
    pub column_span: usize,
}

/// Lay out explicit areas using independently constrained rows and columns.
///
/// Each axis has 1..=32 tracks; at most 256 areas are accepted, and `output`
/// must have exactly that length. Track growth, shrink, caps and trailing unused
/// space follow [`layout_axis`]. Gaps are finite and nonnegative. A spanning area
/// includes all covered tracks and their internal gaps. Overlap is intentional
/// and permitted. Empty area lists still validate and resolve both track axes.
///
/// There is no automatic placement, intrinsic measurement, wrapping, rounding,
/// clipping or pixel scaling. All failures preserve the entire destination.
/// Fixed stack scratch is used without allocation, I/O or window access; this
/// geometry contract does not certify any real-time deadline.
#[allow(clippy::too_many_arguments)] // Explicit independent axes and caller-owned output.
pub fn layout_grid(
    bounds: Rect,
    columns: &[LayoutItem],
    rows: &[LayoutItem],
    column_gap: f64,
    row_gap: f64,
    areas: &[GridArea],
    output: &mut [Rect],
) -> Result<(), LayoutError> {
    if columns.len() > 32 || rows.len() > 32 || areas.len() > 256 || output.len() != areas.len() {
        return Err(LayoutError::Capacity);
    }
    if columns.is_empty() || rows.is_empty() {
        return Err(LayoutError::InvalidInput);
    }
    for area in areas {
        if area.row_span == 0
            || area.column_span == 0
            || area
                .row
                .checked_add(area.row_span)
                .is_none_or(|end| end > rows.len())
            || area
                .column
                .checked_add(area.column_span)
                .is_none_or(|end| end > columns.len())
        {
            return Err(LayoutError::InvalidInput);
        }
    }
    let mut column_bounds = [bounds; 32];
    let mut row_bounds = [bounds; 32];
    layout_axis(
        bounds,
        Axis::Horizontal,
        column_gap,
        columns,
        &mut column_bounds[..columns.len()],
    )?;
    layout_axis(
        bounds,
        Axis::Vertical,
        row_gap,
        rows,
        &mut row_bounds[..rows.len()],
    )?;
    let mut result = [bounds; 256];
    for (index, area) in areas.iter().enumerate() {
        let width = column_bounds[area.column..area.column + area.column_span]
            .iter()
            .map(|track| track.width)
            .sum::<f64>()
            + column_gap * (area.column_span - 1) as f64;
        let height = row_bounds[area.row..area.row + area.row_span]
            .iter()
            .map(|track| track.height)
            .sum::<f64>()
            + row_gap * (area.row_span - 1) as f64;
        result[index] = Rect::new(
            column_bounds[area.column].x,
            row_bounds[area.row].y,
            width,
            height,
        )
        .ok_or(LayoutError::Arithmetic)?;
    }
    output.copy_from_slice(&result[..areas.len()]);
    Ok(())
}
