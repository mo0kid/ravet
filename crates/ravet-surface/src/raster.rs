//! Explicit optional CPU path rasterization, allocating on UI/worker threads.
//!
//! The default-disabled `raster` feature owns premultiplied RGBA8 pixels with
//! finite device geometry and explicit per-frame input/covered-pixel limits.
//! Backend working allocations and execution time remain outside those limits.
//! Combining it with `text` prepares local TrueType glyph outlines separately.
//!
//! ```
//! use ravet_surface::{drawing::{Color, PathCommand}, raster::*};
//! let path = RasterPath::new(&[
//!     PathCommand::MoveTo(1.0, 1.0), PathCommand::LineTo(5.0, 1.0),
//!     PathCommand::LineTo(5.0, 4.0), PathCommand::Close,
//! ], RasterTransform::default())?;
//! let mut canvas = RasterCanvas::new(8, 8, 1)?;
//! canvas.fill_path(&path, Color::rgb(200, 100, 50), RasterFillRule::NonZero, true)?;
//! let before = canvas.snapshot();
//! assert_eq!(canvas.fill_path(&path, Color::rgb(255, 0, 0),
//!     RasterFillRule::NonZero, true), Err(RasterError::Capacity));
//! assert_eq!(canvas.pixels(), before.pixels);
//! # Ok::<(), RasterError>(())
//! ```
use crate::drawing::{Color, PathCommand};
use tiny_skia::{Paint, Path, PathBuilder, Pixmap, Transform};
const MAX_WORK: u64 = 67_108_864;
const MAX_SEGMENTS: usize = 65_536;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RasterError {
    InvalidInput,
    Capacity,
    #[cfg(feature = "text")]
    Text(crate::text::TextError),
}
impl std::fmt::Display for RasterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for RasterError {}
/// Explicit finite affine device transform, applied before f32 quantization.
#[derive(Clone, Copy, Debug)]
pub struct RasterTransform {
    pub scale_x: f64,
    pub skew_x: f64,
    pub translate_x: f64,
    pub skew_y: f64,
    pub scale_y: f64,
    pub translate_y: f64,
}
impl Default for RasterTransform {
    fn default() -> Self {
        Self {
            scale_x: 1.,
            skew_x: 0.,
            translate_x: 0.,
            skew_y: 0.,
            scale_y: 1.,
            translate_y: 0.,
        }
    }
}
impl RasterTransform {
    fn valid(self) -> bool {
        [
            self.scale_x,
            self.skew_x,
            self.translate_x,
            self.skew_y,
            self.scale_y,
            self.translate_y,
        ]
        .iter()
        .all(|v| v.is_finite())
    }
    fn point(self, (x, y): (f64, f64)) -> Result<(f32, f32), RasterError> {
        if !x.is_finite() || !y.is_finite() {
            return Err(RasterError::InvalidInput);
        }
        let x2 = self.scale_x * x + self.skew_x * y + self.translate_x;
        let y2 = self.skew_y * x + self.scale_y * y + self.translate_y;
        if !x2.is_finite() || !y2.is_finite() || x2.abs() > 65536. || y2.abs() > 65536. {
            return Err(RasterError::InvalidInput);
        }
        Ok((x2 as f32, y2 as f32))
    }
}
/// Immutable prepared device-space typed path. UI/worker preparation validates
/// 1..4096 segments, open-subpath grammar, finite transform and resulting control
/// coordinates<=65536 magnitude before f32 conversion. No SVG/URL interpretation.
pub struct RasterPath {
    path: Option<Path>,
    segments: usize,
    bounds: [f64; 4],
}
impl RasterPath {
    pub fn new(commands: &[PathCommand], transform: RasterTransform) -> Result<Self, RasterError> {
        if commands.is_empty() || !transform.valid() {
            return Err(RasterError::InvalidInput);
        }
        if commands.len() > 4096 {
            return Err(RasterError::Capacity);
        }
        let mut builder = PathBuilder::new();
        let mut open = false;
        let mut bounds = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        let mut point = |p| -> Result<(f32, f32), RasterError> {
            let (x, y) = transform.point(p)?;
            let xf = f64::from(x);
            let yf = f64::from(y);
            bounds[0] = bounds[0].min(xf);
            bounds[1] = bounds[1].min(yf);
            bounds[2] = bounds[2].max(xf);
            bounds[3] = bounds[3].max(yf);
            Ok((x, y))
        };
        for command in commands {
            match *command {
                PathCommand::MoveTo(x, y) => {
                    let (x, y) = point((x, y))?;
                    builder.move_to(x, y);
                    open = true;
                }
                PathCommand::LineTo(x, y) if open => {
                    let (x, y) = point((x, y))?;
                    builder.line_to(x, y);
                }
                PathCommand::QuadraticTo { control, end } if open => {
                    let (cx, cy) = point(control)?;
                    let (x, y) = point(end)?;
                    builder.quad_to(cx, cy, x, y);
                }
                PathCommand::CubicTo { first, second, end } if open => {
                    let (x1, y1) = point(first)?;
                    let (x2, y2) = point(second)?;
                    let (x, y) = point(end)?;
                    builder.cubic_to(x1, y1, x2, y2, x, y);
                }
                PathCommand::Close if open => {
                    builder.close();
                    open = false;
                }
                _ => return Err(RasterError::InvalidInput),
            }
        }
        Ok(Self {
            path: builder.finish(),
            segments: commands.len(),
            bounds,
        })
    }
    pub fn segment_count(&self) -> usize {
        self.segments
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RasterFillRule {
    NonZero,
    EvenOdd,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RasterLineCap {
    Butt,
    Round,
    Square,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RasterLineJoin {
    Miter,
    Round,
    Bevel,
}
/// Device-pixel stroke: finite width `(0,256]`, miter limit `[1,16]`. No dashes or zero-width hairlines; positive subpixel antialiased strokes use the backend coverage policy.
#[derive(Clone, Copy, Debug)]
pub struct RasterStroke {
    pub width: f64,
    pub miter_limit: f64,
    pub cap: RasterLineCap,
    pub join: RasterLineJoin,
}
impl Default for RasterStroke {
    fn default() -> Self {
        Self {
            width: 1.,
            miter_limit: 4.,
            cap: RasterLineCap::Butt,
            join: RasterLineJoin::Miter,
        }
    }
}
/// Owned independent top-to-bottom premultiplied RGBA8 numeric readback.
#[derive(Debug)]
pub struct RasterImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}
/// Bounded owned CPU raster canvas, allocating outside callbacks. Each clear
/// starts a frame budget. Source-over numeric components have no implicit gamma/
/// color-space transform. Native window/presenter/device/font loading is separate.
/// Backend working allocations/flattening/time are not globally bounded.
pub struct RasterCanvas {
    pixmap: Pixmap,
    maximum: usize,
    operations: usize,
    segments: usize,
    work: u64,
}
impl RasterCanvas {
    /// Prepare1..4096 dimensions (<=64MiB) and0..4096 raw draw operations per frame.
    /// Clear/covered work<=67108864 and cumulative path segments<=65536. Initial
    /// pixels are transparent; initialization charges complete clear work.
    pub fn new(width: u32, height: u32, maximum_operations: usize) -> Result<Self, RasterError> {
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            return Err(RasterError::InvalidInput);
        }
        if maximum_operations > 4096 {
            return Err(RasterError::Capacity);
        }
        let pixmap = Pixmap::new(width, height).ok_or(RasterError::Capacity)?;
        Ok(Self {
            pixmap,
            maximum: maximum_operations,
            operations: 0,
            segments: 0,
            work: u64::from(width) * u64::from(height),
        })
    }
    pub fn width(&self) -> u32 {
        self.pixmap.width()
    }
    pub fn height(&self) -> u32 {
        self.pixmap.height()
    }
    pub fn pixels(&self) -> &[u8] {
        self.pixmap.data()
    }
    pub fn operation_count(&self) -> usize {
        self.operations
    }
    pub fn segment_count(&self) -> usize {
        self.segments
    }
    pub fn covered_pixel_work(&self) -> u64 {
        self.work
    }
    /// Replace all pixels/reset frame budgets; no implicit clock or presentation.
    pub fn clear(&mut self, color: Color) {
        self.pixmap.fill(tiny_skia::Color::from_rgba8(
            color.red,
            color.green,
            color.blue,
            color.alpha,
        ));
        self.operations = 0;
        self.segments = 0;
        self.work = u64::from(self.width()) * u64::from(self.height());
    }
    fn covered(&self, path: &RasterPath, padding: f64) -> u64 {
        let b = path.bounds;
        let left = (b[0] - padding).clamp(0., f64::from(self.width())).floor();
        let top = (b[1] - padding).clamp(0., f64::from(self.height())).floor();
        let right = (b[2] + padding).clamp(0., f64::from(self.width())).ceil();
        let bottom = (b[3] + padding).clamp(0., f64::from(self.height())).ceil();
        ((right - left).max(0.) as u64) * ((bottom - top).max(0.) as u64)
    }
    fn preflight(&self, segments: usize, work: u64) -> Result<(), RasterError> {
        if self.operations == self.maximum
            || segments > MAX_SEGMENTS - self.segments
            || work > MAX_WORK - self.work
        {
            return Err(RasterError::Capacity);
        }
        Ok(())
    }
    fn accepted(&mut self, segments: usize, work: u64) {
        self.operations += 1;
        self.segments += segments;
        self.work += work;
    }
    fn paint(color: Color, antialias: bool) -> Paint<'static> {
        let mut paint = Paint::default();
        paint.set_color_rgba8(color.red, color.green, color.blue, color.alpha);
        paint.anti_alias = antialias;
        paint
    }
    /// Fill prepared path with explicit rule/antialias switch. Complete budget
    /// rejection preserves all pixels/counters. Degenerate paths consume budgets.
    pub fn fill_path(
        &mut self,
        path: &RasterPath,
        color: Color,
        rule: RasterFillRule,
        antialias: bool,
    ) -> Result<(), RasterError> {
        let work = self.covered(path, if antialias { 1. } else { 0. });
        self.preflight(path.segments, work)?;
        if let Some(path) = &path.path {
            self.pixmap.fill_path(
                path,
                &Self::paint(color, antialias),
                match rule {
                    RasterFillRule::NonZero => tiny_skia::FillRule::Winding,
                    RasterFillRule::EvenOdd => tiny_skia::FillRule::EvenOdd,
                },
                Transform::identity(),
                None,
            );
        }
        self.accepted(path.segments, work);
        Ok(())
    }
    /// Stroke in device pixels with explicit bounded cap/join/miter. Errors preserve
    /// pixels/counters; path transform does not implicitly transform stroke width.
    pub fn stroke_path(
        &mut self,
        path: &RasterPath,
        color: Color,
        style: RasterStroke,
        antialias: bool,
    ) -> Result<(), RasterError> {
        if !style.width.is_finite()
            || style.width <= 0.
            || style.width > 256.
            || (style.width as f32) == 0.
            || !style.miter_limit.is_finite()
            || !(1.0..=16.0).contains(&style.miter_limit)
        {
            return Err(RasterError::InvalidInput);
        }
        let padding = f64::from(style.width as f32)
            * 0.5
            * f64::from(style.miter_limit as f32).max(std::f64::consts::SQRT_2)
            + 1.;
        let work = self.covered(path, padding);
        self.preflight(path.segments, work)?;
        let stroke = tiny_skia::Stroke {
            width: style.width as f32,
            miter_limit: style.miter_limit as f32,
            line_cap: match style.cap {
                RasterLineCap::Butt => tiny_skia::LineCap::Butt,
                RasterLineCap::Round => tiny_skia::LineCap::Round,
                RasterLineCap::Square => tiny_skia::LineCap::Square,
            },
            line_join: match style.join {
                RasterLineJoin::Miter => tiny_skia::LineJoin::Miter,
                RasterLineJoin::Round => tiny_skia::LineJoin::Round,
                RasterLineJoin::Bevel => tiny_skia::LineJoin::Bevel,
            },
            ..Default::default()
        };
        if let Some(path) = &path.path {
            self.pixmap.stroke_path(
                path,
                &Self::paint(color, antialias),
                &stroke,
                Transform::identity(),
                None,
            );
        }
        self.accepted(path.segments, work);
        Ok(())
    }
    /// Copy an independent owned pixel readback with the same dimensions.
    pub fn snapshot(&self) -> RasterImage {
        RasterImage {
            width: self.width(),
            height: self.height(),
            pixels: self.pixels().to_vec(),
        }
    }
    /// Transfer the pixel allocation without copying; a presenter validates adoption.
    pub fn into_image(self) -> RasterImage {
        RasterImage {
            width: self.width(),
            height: self.height(),
            pixels: self.pixmap.take(),
        }
    }
}

#[cfg(feature = "text")]
#[path = "raster_text.rs"]
mod raster_text;
#[cfg(feature = "text")]
pub use raster_text::RasterText;
