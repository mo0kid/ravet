//! Bounded local SVG drawing for explicit UI/control-thread use.
use crate::layout::Rect;
use std::fmt::Write as _;
/// Straight RGBA color. No implicit color-space conversion is performed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}
impl Color {
    pub const fn rgb(red: u8, green: u8, blue: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha: 255,
        }
    }
    fn paint(self) -> String {
        format!(
            "rgba({},{},{},{})",
            self.red,
            self.green,
            self.blue,
            f64::from(self.alpha) / 255.
        )
    }
}
/// Consumer-replaceable semantic colors, independent of the window backend.
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub background: Color,
    pub panel: Color,
    pub foreground: Color,
    pub muted: Color,
    pub accent: Color,
}
impl Theme {
    /// Default warm charcoal/plum palette with amber emphasis.
    pub const DARK: Self = Self {
        background: Color::rgb(23, 21, 29),
        panel: Color::rgb(37, 33, 45),
        foreground: Color::rgb(242, 236, 226),
        muted: Color::rgb(179, 167, 188),
        accent: Color::rgb(243, 182, 93),
    };
    /// Companion warm paper palette with deep amber emphasis.
    pub const LIGHT: Self = Self {
        background: Color::rgb(248, 244, 236),
        panel: Color::rgb(235, 226, 217),
        foreground: Color::rgb(23, 21, 29),
        muted: Color::rgb(99, 84, 105),
        accent: Color::rgb(143, 81, 5),
    };
}
/// Explicit text styling; family selects a consumer-installed/system font.
/// This font-family primitive delegates loading/measurement to SVG. Separate
/// optional local-font APIs provide shaped outlines.
#[derive(Clone, Copy, Debug)]
pub struct Font<'a> {
    pub family: &'a str,
    pub size: f64,
    pub weight: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAnchor {
    Start,
    Middle,
    End,
}
/// Typed filled-path segments. Coordinates/control points are checked before
/// appending; no arbitrary SVG markup/URL or transform string is accepted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathCommand {
    MoveTo(f64, f64),
    LineTo(f64, f64),
    QuadraticTo {
        control: (f64, f64),
        end: (f64, f64),
    },
    CubicTo {
        first: (f64, f64),
        second: (f64, f64),
        end: (f64, f64),
    },
    Close,
}
#[cfg(feature = "text")]
impl PathCommand {
    pub(crate) fn coordinates_are_finite(self) -> bool {
        let finite = |(x, y): (f64, f64)| x.is_finite() && y.is_finite();
        match self {
            Self::MoveTo(x, y) | Self::LineTo(x, y) => finite((x, y)),
            Self::QuadraticTo { control, end } => finite(control) && finite(end),
            Self::CubicTo { first, second, end } => finite(first) && finite(second) && finite(end),
            Self::Close => true,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawingError {
    InvalidInput,
    Capacity,
}
fn text_valid(text: &str, maximum: usize) -> bool {
    text.len() <= maximum
        && !text
            .chars()
            .any(|c| c.is_control() || c == '\u{fffe}' || c == '\u{ffff}')
}
fn escaped(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&apos;"),
            _ => result.push(character),
        }
    }
    result
}
/// An owned SVG document with at most 4096 drawing commands/1 MiB serialized data.
/// It embeds no script, external resource, arbitrary markup or URL. Geometry uses
/// caller-selected viewBox units; text uses escaped content and native SVG font
/// fallback. UI allocations/formatting occur on each accepted command. Capacity
/// or input rejection leaves the document unchanged. Optional `text` adds local
/// shaped glyf outlines. No GPU/window/raster export, hit testing or
/// assistive-technology validation is supplied.
pub struct SvgDrawing {
    document: String,
    commands: usize,
}
impl SvgDrawing {
    /// Create a positive finite viewport with an escaped accessible title <=4096
    /// bytes. The title supplies role=img labeling; individual shapes are visual.
    pub fn new(width: f64, height: f64, title: &str) -> Result<Self, DrawingError> {
        if ![width, height].iter().all(|v| v.is_finite() && *v > 0.) || !text_valid(title, 4096) {
            return Err(DrawingError::InvalidInput);
        }
        let document = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {width} {height}\" role=\"img\" aria-label=\"{}\"><title>{}</title>",
            escaped(title),
            escaped(title)
        );
        Ok(Self {
            document,
            commands: 0,
        })
    }
    fn append(&mut self, command: String) -> Result<(), DrawingError> {
        if self.commands >= 4096 || self.document.len() + command.len() + 6 > 1_048_576 {
            return Err(DrawingError::Capacity);
        }
        self.document.push_str(&command);
        self.commands += 1;
        Ok(())
    }
    /// Draw a filled rectangle, clamping nonnegative corner radius to half extent.
    pub fn rectangle(
        &mut self,
        bounds: Rect,
        radius: f64,
        color: Color,
    ) -> Result<(), DrawingError> {
        if !radius.is_finite() || radius < 0. {
            return Err(DrawingError::InvalidInput);
        }
        let radius = radius.min(bounds.width() / 2.).min(bounds.height() / 2.);
        self.append(format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{radius}\" fill=\"{}\"/>",
            bounds.x(),
            bounds.y(),
            bounds.width(),
            bounds.height(),
            color.paint()
        ))
    }
    /// Draw 2..4096 finite points with a positive finite width and rounded joins.
    pub fn polyline(
        &mut self,
        points: &[(f64, f64)],
        width: f64,
        color: Color,
    ) -> Result<(), DrawingError> {
        if !(2..=4096).contains(&points.len())
            || !width.is_finite()
            || width <= 0.
            || points.iter().any(|(x, y)| !x.is_finite() || !y.is_finite())
        {
            return Err(DrawingError::InvalidInput);
        }
        let mut command = String::from("<polyline points=\"");
        for (x, y) in points {
            write!(command, "{x},{y} ").unwrap();
        }
        write!(
            command,
            "\" fill=\"none\" stroke=\"{}\" stroke-width=\"{width}\" stroke-linejoin=\"round\"/>",
            color.paint()
        )
        .unwrap();
        self.append(command)
    }
    /// Append one filled path of 1..4096 typed commands. It must start with
    /// MoveTo; drawing/Close needs an open subpath, and Close requires a new
    /// MoveTo before further segments. All coordinates are finite. Rejection
    /// leaves the document unchanged. SVG's normal implicit fill closure applies.
    pub fn path(&mut self, commands: &[PathCommand], color: Color) -> Result<(), DrawingError> {
        if commands.is_empty() {
            return Err(DrawingError::InvalidInput);
        }
        if commands.len() > 4096 {
            return Err(DrawingError::Capacity);
        }
        let mut open = false;
        let mut command = String::from("<path d=\"");
        for segment in commands {
            let finite = |point: (f64, f64)| point.0.is_finite() && point.1.is_finite();
            match *segment {
                PathCommand::MoveTo(x, y) if finite((x, y)) => {
                    write!(command, "M{x} {y} ").unwrap();
                    open = true;
                }
                PathCommand::LineTo(x, y) if open && finite((x, y)) => {
                    write!(command, "L{x} {y} ").unwrap();
                }
                PathCommand::QuadraticTo {
                    control: (cx, cy),
                    end: (x, y),
                } if open && finite((cx, cy)) && finite((x, y)) => {
                    write!(command, "Q{cx} {cy} {x} {y} ").unwrap();
                }
                PathCommand::CubicTo {
                    first: (cx0, cy0),
                    second: (cx1, cy1),
                    end: (x, y),
                } if open && finite((cx0, cy0)) && finite((cx1, cy1)) && finite((x, y)) => {
                    write!(command, "C{cx0} {cy0} {cx1} {cy1} {x} {y} ").unwrap();
                }
                PathCommand::Close if open => {
                    command.push_str("Z ");
                    open = false;
                }
                _ => return Err(DrawingError::InvalidInput),
            }
        }
        write!(command, "\" fill=\"{}\"/>", color.paint()).unwrap();
        self.append(command)
    }
    /// Render one shaped local TrueType glyf run as filled outline paths at a
    /// caller baseline (SVG Y down). Whole-run failure restores the document.
    /// No text/font URL is emitted; accessible meaning remains the drawing title.
    /// Spaces/empty outlines draw nothing; missing glyph zero draws that font's
    /// replacement outline. CFF/Type1/color/bitmap glyph rendering is unsupported.
    #[cfg(feature = "text")]
    pub fn shaped_text(
        &mut self,
        font: &crate::text::TextFont,
        text: &str,
        options: crate::text::TextOptions<'_>,
        baseline: (f64, f64),
        color: Color,
    ) -> Result<(), crate::text::TextError> {
        use crate::text::TextError;
        if !baseline.0.is_finite() || !baseline.1.is_finite() {
            return Err(TextError::InvalidInput);
        }
        let run = font.shape(text, options)?;
        let scale = options.size / f64::from(font.units_per_em());
        let saved = (self.document.len(), self.commands);
        let result = (|| {
            for glyph in run.glyphs() {
                let transform = |(x, y): (f64, f64)| {
                    (
                        baseline.0 + glyph.x + x * scale,
                        baseline.1 - glyph.y - y * scale,
                    )
                };
                let path = font
                    .outline_glyph(glyph.id)?
                    .into_iter()
                    .map(|segment| match segment {
                        PathCommand::MoveTo(x, y) => {
                            let (x, y) = transform((x, y));
                            PathCommand::MoveTo(x, y)
                        }
                        PathCommand::LineTo(x, y) => {
                            let (x, y) = transform((x, y));
                            PathCommand::LineTo(x, y)
                        }
                        PathCommand::QuadraticTo { control, end } => PathCommand::QuadraticTo {
                            control: transform(control),
                            end: transform(end),
                        },
                        PathCommand::CubicTo { first, second, end } => PathCommand::CubicTo {
                            first: transform(first),
                            second: transform(second),
                            end: transform(end),
                        },
                        PathCommand::Close => PathCommand::Close,
                    })
                    .collect::<Vec<_>>();
                if !path.is_empty() {
                    self.path(&path, color).map_err(|error| match error {
                        DrawingError::InvalidInput => TextError::InvalidInput,
                        DrawingError::Capacity => TextError::Capacity,
                    })?;
                }
            }
            Ok(())
        })();
        if result.is_err() {
            self.document.truncate(saved.0);
            self.commands = saved.1;
        }
        result
    }
    /// Draw escaped text <=4096 bytes at a baseline. Font family <=128 nonempty
    /// bytes, finite positive size and CSS numeric weight 1..1000 are required.
    pub fn text(
        &mut self,
        position: (f64, f64),
        text: &str,
        font: Font<'_>,
        anchor: TextAnchor,
        color: Color,
    ) -> Result<(), DrawingError> {
        if !position.0.is_finite()
            || !position.1.is_finite()
            || !text_valid(text, 4096)
            || !text_valid(font.family, 128)
            || font.family.is_empty()
            || !font.size.is_finite()
            || font.size <= 0.
            || !(1..=1000).contains(&font.weight)
        {
            return Err(DrawingError::InvalidInput);
        }
        let anchor = match anchor {
            TextAnchor::Start => "start",
            TextAnchor::Middle => "middle",
            TextAnchor::End => "end",
        };
        self.append(format!("<text x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" font-weight=\"{}\" text-anchor=\"{anchor}\" fill=\"{}\">{}</text>",position.0,position.1,escaped(font.family),font.size,font.weight,color.paint(),escaped(text)))
    }
    pub fn command_count(&self) -> usize {
        self.commands
    }
    /// Complete and consume the drawing. No file/browser action is performed.
    pub fn finish(mut self) -> String {
        self.document.push_str("</svg>");
        self.document
    }
}
