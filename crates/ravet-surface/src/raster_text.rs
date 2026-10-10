use super::{MAX_SEGMENTS, RasterCanvas, RasterError, RasterPath, RasterTransform};
use crate::{
    drawing::Color,
    text::{ShapedText, TextFont},
};
use tiny_skia::Transform;
/// Independent prepared TrueType glyph geometry in device coordinates; no font
/// or source borrow survives preparation. Caller supplies the matching font/run
/// and shaping size. Missing glyph zero uses that font's replacement outline;
/// CFF/Type1/color/bitmap glyphs or system fallback are not inferred.
pub struct RasterText {
    paths: Vec<RasterPath>,
    segments: usize,
    missing: usize,
}
impl RasterText {
    /// Prepare the complete cached run (<=16384 glyphs), size(0,1024] and finite
    /// baseline. Resulting glyph path/control device coordinates remain within
    ///65536 magnitude, each path<=4096 commands and whole run<=65536 commands.
    /// Font extraction/geometry can allocate/block; no native input or deadline.
    pub fn from_shaped(
        font: &TextFont,
        run: &ShapedText,
        size: f64,
        baseline: (f64, f64),
    ) -> Result<Self, RasterError> {
        if !size.is_finite()
            || size <= 0.
            || size > 1024.
            || !baseline.0.is_finite()
            || !baseline.1.is_finite()
        {
            return Err(RasterError::InvalidInput);
        }
        if run.glyphs().len() > 16384 {
            return Err(RasterError::Capacity);
        }
        let scale = size / f64::from(font.units_per_em());
        let mut paths = Vec::with_capacity(run.glyphs().len());
        let mut segments = 0;
        for glyph in run.glyphs() {
            let commands = font.outline_glyph(glyph.id).map_err(RasterError::Text)?;
            if commands.len() > MAX_SEGMENTS - segments {
                return Err(RasterError::Capacity);
            }
            if commands.is_empty() {
                continue;
            }
            let transform = RasterTransform {
                scale_x: scale,
                scale_y: -scale,
                translate_x: baseline.0 + glyph.x,
                translate_y: baseline.1 - glyph.y,
                ..Default::default()
            };
            let path = RasterPath::new(&commands, transform)?;
            segments += path.segment_count();
            paths.push(path);
        }
        Ok(Self {
            paths,
            segments,
            missing: run.missing_glyphs(),
        })
    }
    pub fn segment_count(&self) -> usize {
        self.segments
    }
    pub fn missing_glyphs(&self) -> usize {
        self.missing
    }
}
impl RasterCanvas {
    /// Draw one complete prepared TrueType run with nonzero fill and explicit
    /// antialiasing. Whole run budget preflight precedes pixel/counter mutation.
    /// One run consumes one raw operation, including empty/space-only runs.
    pub fn draw_text(
        &mut self,
        text: &RasterText,
        color: Color,
        antialias: bool,
    ) -> Result<(), RasterError> {
        let mut work = 0;
        for path in &text.paths {
            work += self.covered(path, if antialias { 1. } else { 0. });
            self.preflight(text.segments, work)?;
        }
        self.preflight(text.segments, work)?;
        let paint = Self::paint(color, antialias);
        for path in &text.paths {
            if let Some(path) = &path.path {
                self.pixmap.fill_path(
                    path,
                    &paint,
                    tiny_skia::FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }
        self.accepted(text.segments, work);
        Ok(())
    }
}
