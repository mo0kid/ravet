use super::{MAX_SEGMENTS, RasterCanvas, RasterError, RasterPath, RasterTransform};
use crate::{
    drawing::Color,
    text::{ShapedText, TextFont},
};
use tiny_skia::Transform;
/// Independent prepared admitted glyph geometry in device coordinates; no font
/// or source borrow survives preparation. Caller supplies the matching font/run
/// and shaping size. Missing glyph zero uses that font's replacement outline;
/// Optional postscript-outlines explicitly admits selected CFF1/CFF2;
/// Type1/color/bitmap glyphs or system fallback are not inferred.
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
    /// Prepare a complete fallback run with its retained exact matching faces.
    /// Aggregate paths obey the same65536-command geometry cap before publication;
    /// no caller canvas pixels or counters change on failure.
    pub fn from_fallback(
        text: &crate::fallback::FallbackText<'_>,
        baseline: (f64, f64),
    ) -> Result<Self, RasterError> {
        if !baseline.0.is_finite() || !baseline.1.is_finite() {
            return Err(RasterError::InvalidInput);
        }
        let mut paths = Vec::new();
        let mut segments = 0;
        for run in text.runs() {
            let origin = run.origin();
            let prepared = Self::from_shaped(
                run.font(),
                run.shape(),
                text.size(),
                (baseline.0 + origin.0, baseline.1 - origin.1),
            )?;
            if prepared.segments > MAX_SEGMENTS - segments {
                return Err(RasterError::Capacity);
            }
            segments += prepared.segments;
            paths.extend(prepared.paths);
        }
        Ok(Self {
            paths,
            segments,
            missing: text.missing_glyphs(),
        })
    }
    /// Prepare a complete mixed-bidi fallback paragraph with its exact selected
    /// faces. One owned geometry output obeys aggregate65536-command/device bounds
    /// before publication; no font/layout borrow or canvas mutation survives here.
    pub fn from_fallback_paragraph(
        layout: &crate::fallback_paragraph::FallbackParagraphLayout<'_>,
        baseline: (f64, f64),
    ) -> Result<Self, RasterError> {
        if !baseline.0.is_finite() || !baseline.1.is_finite() {
            return Err(RasterError::InvalidInput);
        }
        let mut paths = Vec::new();
        let mut segments = 0;
        for line in layout.lines() {
            for run in line.runs() {
                let prepared = Self::from_shaped(
                    run.font(),
                    run.shape(),
                    layout.size(),
                    (
                        baseline.0 + line.baseline().0 + run.x(),
                        baseline.1 + line.baseline().1,
                    ),
                )?;
                if prepared.segments > MAX_SEGMENTS - segments {
                    return Err(RasterError::Capacity);
                }
                segments += prepared.segments;
                paths.extend(prepared.paths);
            }
        }
        Ok(Self {
            paths,
            segments,
            missing: layout.missing_glyphs(),
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
    /// Draw one complete prepared outline run with nonzero fill and explicit
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
