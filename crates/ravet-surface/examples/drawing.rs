//! Print a standalone SVG exercising layout, themed typography and scalar motion.
use ravet_surface::{
    animation::{Easing, Tween},
    drawing::{Font, SvgDrawing, TextAnchor, Theme},
    layout::{Axis, LayoutItem, Rect, layout_axis},
};
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let width: f64 = args
        .first()
        .map(|v| v.parse().expect("finite width"))
        .unwrap_or(640.);
    let time: u64 = args
        .get(1)
        .map(|v| v.parse().expect("time in milliseconds"))
        .unwrap_or(500);
    let theme = if args.get(2).is_some_and(|v| v == "light") {
        Theme::LIGHT
    } else {
        Theme::DARK
    };
    assert!((360. ..=1600.).contains(&width), "width must be 360..1600");
    let viewport = Rect::new(0., 0., width, 300.).unwrap();
    let mut drawing = SvgDrawing::new(width, 300., "RAVET layout and animation reference").unwrap();
    drawing.rectangle(viewport, 0., theme.background).unwrap();
    let mut rows = [viewport; 3];
    layout_axis(
        viewport.inset(24.).unwrap(),
        Axis::Vertical,
        16.,
        &[
            LayoutItem::fixed(40.),
            LayoutItem::fixed(156.),
            LayoutItem::fixed(24.),
        ],
        &mut rows,
    )
    .unwrap();
    let title = Font {
        family: "system-ui, sans-serif",
        size: 24.,
        weight: 650,
    };
    let label = Font {
        size: 14.,
        weight: 400,
        ..title
    };
    drawing
        .text(
            (rows[0].x(), rows[0].y() + 28.),
            "RAVET · Layout & motion",
            title,
            TextAnchor::Start,
            theme.foreground,
        )
        .unwrap();
    let mut panels = [viewport; 2];
    let item = LayoutItem {
        minimum: 140.,
        preferred: 140.,
        maximum: 1600.,
        grow: 1.,
    };
    layout_axis(rows[1], Axis::Horizontal, 16., &[item, item], &mut panels).unwrap();
    let tween = Tween::new(0., 1., 0, 1000, Easing::SmoothStep).unwrap();
    for (index, panel) in panels.iter().enumerate() {
        drawing.rectangle(*panel, 12., theme.panel).unwrap();
        let inner = panel.inset(16.).unwrap();
        drawing
            .text(
                (inner.x(), inner.y() + 18.),
                if index == 0 { "Input" } else { "Output" },
                label,
                TextAnchor::Start,
                theme.foreground,
            )
            .unwrap();
        let y = inner.y() + 48.;
        let ratio = tween.sample(time) * if index == 0 { 0.75 } else { 1. };
        drawing
            .rectangle(
                Rect::new(inner.x(), y, inner.width(), 16.).unwrap(),
                8.,
                theme.background,
            )
            .unwrap();
        drawing
            .rectangle(
                Rect::new(inner.x(), y, inner.width() * ratio, 16.).unwrap(),
                8.,
                theme.accent,
            )
            .unwrap();
        let points: Vec<_> = (0..48)
            .map(|i| {
                let x = inner.x() + inner.width() * f64::from(i) / 47.;
                let y = inner.y() + 100. + (f64::from(i) * 0.4).sin() * 14. * ratio;
                (x, y)
            })
            .collect();
        drawing.polyline(&points, 2., theme.accent).unwrap();
    }
    drawing
        .text(
            (rows[2].x(), rows[2].y() + 18.),
            "Synthetic display · explicit clock · local SVG",
            label,
            TextAnchor::Start,
            theme.muted,
        )
        .unwrap();
    println!("{}", drawing.finish());
}
