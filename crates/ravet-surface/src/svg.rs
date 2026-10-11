//! Owned worker SVG import with a fixed resource-free shape/paint profile.
//!
//! The default-disabled `svg-image` feature parses at most1MiB UTF8,8192 XML
//! nodes,4096 elements,32 levels and65536 source/normalized path segments.
//! Shapes, transforms, inline presentation, gradients and non-nested clip paths
//! are explicit. Text/fonts, images/use, CSS stylesheets, animation, filters,
//! masks, patterns, DTD and all external resources are rejected. Backend working
//! allocations/normalization/flattening/time are not globally bounded.
//!
//! Output is owned top-to-bottom sRGB-encoded premultiplied RGBA8,1..4096 pixels
//! per dimension/64MiB. Alpha association and numeric source-over are separate
//! from linear-light color preparation and display/ICC management.
//! ```
//! use ravet_surface::svg::{SvgDocument, SvgFit};
//! let source = br##"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><rect width="2" height="2" fill="#804020"/></svg>"##;
//! let document = SvgDocument::parse(source)?;
//! let image = document.render(2, 2, SvgFit::Contain)?;
//! assert_eq!(&image.pixels()[..4], &[128, 64, 32, 255]);
//! # Ok::<(), ravet_surface::svg::SvgError>(())
//! ```
use resvg::{tiny_skia, usvg};
use roxmltree::{Document, Node};
use std::collections::BTreeMap;
use std::str::FromStr;
const NS: &str = "http://www.w3.org/2000/svg";
const MAX_SOURCE: usize = 1024 * 1024;
const MAX_COORD: f64 = 65536.;
/// Complete input/profile/capacity rejection; no partial image is published.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SvgError {
    InvalidInput,
    Unsupported,
    Capacity,
}
impl std::fmt::Display for SvgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SvgError {}
/// Outer fitting of the SVG's resolved viewport, after its own viewBox mapping.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SvgFit {
    #[default]
    Contain,
    Stretch,
}
/// Immutable owned normalized SVG; no source, file, font or window ownership.
pub struct SvgDocument {
    tree: usvg::Tree,
}
/// Independent immutable sRGB-encoded premultiplied RGBA8 with validated shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SvgImage {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}
impl SvgImage {
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
    /// Transfer packed pixels; source-over remains backend numeric encoded math.
    pub fn into_premultiplied_rgba(self) -> Vec<u8> {
        self.pixels
    }
}
impl SvgDocument {
    /// Parse a fixed shape/paint profile, refusing unsupported/resources before
    /// backend conversion. Private normalized data remains independent of source.
    pub fn parse(source: &[u8]) -> Result<Self, SvgError> {
        if source.is_empty() {
            return Err(SvgError::InvalidInput);
        }
        if source.len() > MAX_SOURCE {
            return Err(SvgError::Capacity);
        }
        let text = std::str::from_utf8(source).map_err(|_| SvgError::InvalidInput)?;
        // Even an empty DTD is prohibited; roxmltree's allow_dtd=false permits it.
        if text.contains("<!DOCTYPE") {
            return Err(SvgError::Unsupported);
        }
        let doc = Document::parse_with_options(
            text,
            roxmltree::ParsingOptions {
                allow_dtd: false,
                nodes_limit: 8192,
                entity_resolver: None,
            },
        )
        .map_err(|e| {
            if matches!(e, roxmltree::Error::NodesLimitReached) {
                SvgError::Capacity
            } else {
                SvgError::InvalidInput
            }
        })?;
        preflight(&doc)?;
        let options = usvg::Options {
            resources_dir: None,
            style_sheet: None,
            image_href_resolver: usvg::ImageHrefResolver {
                resolve_data: Box::new(|_, _, _| None),
                resolve_string: Box::new(|_, _| None),
            },
            ..Default::default()
        };
        let tree = usvg::Tree::from_xmltree(&doc, &options).map_err(|_| SvgError::InvalidInput)?;
        let size = tree.size();
        if !finite_positive(f64::from(size.width())) || !finite_positive(f64::from(size.height())) {
            return Err(SvgError::InvalidInput);
        }
        normalized(&tree, tiny_skia::Transform::identity())?;
        Ok(Self { tree })
    }
    /// Resolved root viewport, independent of painted bounds and consumer window.
    pub fn intrinsic_size(&self) -> (f64, f64) {
        let s = self.tree.size();
        (f64::from(s.width()), f64::from(s.height()))
    }
    /// Prepare transparent1..4096/64MiB pixels with explicit outer fitting. All
    /// geometry/output admission precedes allocation and complete publication.
    pub fn render(&self, width: u32, height: u32, fit: SvgFit) -> Result<SvgImage, SvgError> {
        if width == 0 || height == 0 {
            return Err(SvgError::InvalidInput);
        }
        if width > 4096 || height > 4096 {
            return Err(SvgError::Capacity);
        }
        let (iw, ih) = self.intrinsic_size();
        let sx = f64::from(width) / iw;
        let sy = f64::from(height) / ih;
        let (sx, sy, tx, ty) = match fit {
            SvgFit::Stretch => (sx, sy, 0., 0.),
            SvgFit::Contain => {
                let s = sx.min(sy);
                (
                    s,
                    s,
                    (f64::from(width) - iw * s) / 2.,
                    (f64::from(height) - ih * s) / 2.,
                )
            }
        };
        let t = tiny_skia::Transform::from_row(sx as f32, 0., 0., sy as f32, tx as f32, ty as f32);
        normalized(&self.tree, t)?;
        let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or(SvgError::Capacity)?;
        resvg::render(&self.tree, t, &mut pixmap.as_mut());
        let pixels = pixmap.take();
        if pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[..3].iter().any(|c| *c > p[3]))
        {
            return Err(SvgError::InvalidInput);
        }
        Ok(SvgImage {
            width,
            height,
            pixels,
        })
    }
}
fn finite_positive(n: f64) -> bool {
    n.is_finite() && n > 0. && n <= MAX_COORD
}
fn coord(n: f64) -> Result<(), SvgError> {
    if n.is_finite() && n.abs() <= MAX_COORD {
        Ok(())
    } else {
        Err(SvgError::InvalidInput)
    }
}
fn number(v: &str) -> Result<f64, SvgError> {
    let n = svgtypes::Number::from_str(v.trim())
        .map_err(|_| SvgError::InvalidInput)?
        .0;
    coord(n)?;
    Ok(n)
}
fn length(v: &str, percent: bool) -> Result<f64, SvgError> {
    let n = svgtypes::Length::from_str(v.trim()).map_err(|_| SvgError::InvalidInput)?;
    if !matches!(
        n.unit,
        svgtypes::LengthUnit::None | svgtypes::LengthUnit::Px
    ) && !(percent && n.unit == svgtypes::LengthUnit::Percent)
    {
        return Err(SvgError::Unsupported);
    }
    coord(n.number)?;
    Ok(if n.unit == svgtypes::LengthUnit::Percent {
        n.number / 100.
    } else {
        n.number
    })
}
fn choice(v: &str, allowed: &[&str]) -> Result<(), SvgError> {
    if allowed.contains(&v.trim()) {
        Ok(())
    } else {
        Err(SvgError::InvalidInput)
    }
}
fn ascii_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}
fn whitespace(v: &str) -> bool {
    v.bytes().all(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
}
fn shape(n: &str) -> bool {
    matches!(
        n,
        "path" | "rect" | "circle" | "ellipse" | "line" | "polyline" | "polygon"
    )
}
#[derive(Clone, Copy, PartialEq)]
enum RefKind {
    Paint,
    Clip,
}
#[derive(Default)]
struct Budget {
    segments: usize,
    stops: usize,
}
fn preflight(doc: &Document<'_>) -> Result<(), SvgError> {
    let root = doc.root_element();
    if root.tag_name().namespace() != Some(NS) || root.tag_name().name() != "svg" {
        return Err(SvgError::Unsupported);
    }
    let mut ids = BTreeMap::new();
    let mut refs = Vec::new();
    let mut budget = Budget::default();
    let mut elements = 0;
    for node in doc.descendants() {
        if node.is_pi() {
            return Err(SvgError::Unsupported);
        }
        if node.is_text() {
            let parent = node
                .parent()
                .filter(Node::is_element)
                .map(|n| n.tag_name().name());
            if !matches!(parent, Some("title" | "desc")) && !whitespace(node.text().unwrap_or("")) {
                return Err(SvgError::Unsupported);
            }
            continue;
        }
        if !node.is_element() {
            continue;
        }
        elements += 1;
        if elements > 4096 || node.ancestors().filter(Node::is_element).count() > 32 {
            return Err(SvgError::Capacity);
        }
        let tag = node.tag_name().name();
        if node.tag_name().namespace() != Some(NS)
            || !(shape(tag)
                || matches!(
                    tag,
                    "svg"
                        | "g"
                        | "defs"
                        | "linearGradient"
                        | "radialGradient"
                        | "stop"
                        | "clipPath"
                        | "title"
                        | "desc"
                ))
        {
            return Err(SvgError::Unsupported);
        }
        if node != root && tag == "svg" {
            return Err(SvgError::Unsupported);
        }
        if let Some(parent) = node.parent().filter(Node::is_element) {
            let p = parent.tag_name().name();
            let allowed = match p {
                "svg" | "g" | "defs" => tag != "stop",
                "linearGradient" | "radialGradient" => tag == "stop",
                "clipPath" => shape(tag) || tag == "g",
                _ => false,
            };
            if !allowed {
                return Err(SvgError::Unsupported);
            }
        }
        let in_clip = node
            .ancestors()
            .any(|n| n.is_element() && n.tag_name().name() == "clipPath");
        if in_clip && !(shape(tag) || matches!(tag, "g" | "clipPath")) {
            return Err(SvgError::Unsupported);
        }
        if node.attributes().len() > 64 || node.namespaces().len() > 64 {
            return Err(SvgError::Capacity);
        }
        if let Some(id) = node.attribute("id") {
            if !ascii_id(id) || ids.insert(id, tag).is_some() {
                return Err(SvgError::InvalidInput);
            }
            if ids.len() > 1024 {
                return Err(SvgError::Capacity);
            }
        }
        if tag == "stop" {
            budget.stops += 1;
            if budget.stops > 4096 {
                return Err(SvgError::Capacity);
            }
        }
        if matches!(tag, "linearGradient" | "radialGradient")
            && node.children().filter(Node::is_element).count() > 256
        {
            return Err(SvgError::Capacity);
        }
        for attr in node.attributes() {
            if attr.value().len() > 65536 {
                return Err(SvgError::Capacity);
            }
            if attr.namespace().is_some() {
                return Err(SvgError::Unsupported);
            }
            attribute(
                tag,
                attr.name(),
                attr.value(),
                in_clip,
                &mut refs,
                &mut budget,
            )?;
        }
    }
    for (id, kind) in refs {
        let target = ids.get(id).ok_or(SvgError::InvalidInput)?;
        if match kind {
            RefKind::Paint => !matches!(*target, "linearGradient" | "radialGradient"),
            RefKind::Clip => *target != "clipPath",
        } {
            return Err(SvgError::InvalidInput);
        }
    }
    // Eliminate environment-dependent root fallback and geometry dimensions.
    let view = root.attribute("viewBox").map(view_box).transpose()?;
    for name in ["width", "height"] {
        if let Some(v) = root.attribute(name) {
            if !finite_positive(length(v, false)?) {
                return Err(SvgError::InvalidInput);
            }
        } else if view.is_none() {
            return Err(SvgError::InvalidInput);
        }
    }
    Ok(())
}
fn view_box(v: &str) -> Result<svgtypes::ViewBox, SvgError> {
    let b = svgtypes::ViewBox::from_str(v).map_err(|_| SvgError::InvalidInput)?;
    coord(b.x)?;
    coord(b.y)?;
    if !finite_positive(b.w) || !finite_positive(b.h) {
        return Err(SvgError::InvalidInput);
    }
    Ok(b)
}
fn attribute<'a>(
    tag: &str,
    name: &str,
    value: &'a str,
    in_clip: bool,
    refs: &mut Vec<(&'a str, RefKind)>,
    budget: &mut Budget,
) -> Result<(), SvgError> {
    if name == "id" {
        return if ascii_id(value) {
            Ok(())
        } else {
            Err(SvgError::InvalidInput)
        };
    }
    if matches!(name, "role" | "aria-label" | "aria-labelledby") {
        return Ok(());
    }
    if name == "version" && tag == "svg" {
        return choice(value, &["1.0", "1.1", "2.0"]);
    }
    if name == "style" {
        return style(value, in_clip, refs);
    }
    if presentation(name, value, in_clip, refs)?.is_some() {
        return Ok(());
    }
    match (tag, name) {
        ("svg", "viewBox") => {
            view_box(value)?;
        }
        ("svg", "preserveAspectRatio") => {
            let a = svgtypes::AspectRatio::from_str(value).map_err(|_| SvgError::InvalidInput)?;
            if a.defer {
                return Err(SvgError::Unsupported);
            }
        }
        ("svg" | "rect", "width" | "height")
        | ("rect", "rx" | "ry")
        | ("circle", "r")
        | ("ellipse", "rx" | "ry") => {
            if length(value, false)? < 0. {
                return Err(SvgError::InvalidInput);
            }
        }
        ("rect", "x" | "y")
        | ("circle" | "ellipse", "cx" | "cy")
        | ("line", "x1" | "x2" | "y1" | "y2") => {
            length(value, false)?;
        }
        ("path", "d") => path(value, budget)?,
        ("polyline" | "polygon", "points") => {
            let mut count = 0;
            for p in svgtypes::PointsParser::from(value) {
                coord(p.0)?;
                coord(p.1)?;
                count += 1;
                if count > 4096 {
                    return Err(SvgError::Capacity);
                }
            }
            if count == 0 {
                return Err(SvgError::InvalidInput);
            } // Full lexical coverage independently checked because PointsParser silently stops.
            let nums = number_list(value)?;
            if nums.len() != count * 2 {
                return Err(SvgError::InvalidInput);
            }
            budget.segments += count;
            if budget.segments > 65536 {
                return Err(SvgError::Capacity);
            }
        }
        ("linearGradient", "x1" | "x2" | "y1" | "y2")
        | ("radialGradient", "cx" | "cy" | "fx" | "fy") => {
            length(value, true)?;
        }
        ("radialGradient", "r") => {
            if length(value, true)? < 0. {
                return Err(SvgError::InvalidInput);
            }
        }
        ("linearGradient" | "radialGradient", "gradientUnits") | ("clipPath", "clipPathUnits") => {
            choice(value, &["userSpaceOnUse", "objectBoundingBox"])?
        }
        ("linearGradient" | "radialGradient", "spreadMethod") => {
            choice(value, &["pad", "reflect", "repeat"])?
        }
        ("linearGradient" | "radialGradient", "gradientTransform") => transform(value)?,
        ("stop", "offset") => {
            let n = length(value, true)?;
            if !(0. ..=1.).contains(&n) {
                return Err(SvgError::InvalidInput);
            }
        }
        _ => return Err(SvgError::Unsupported),
    };
    Ok(())
}
fn style<'a>(
    v: &'a str,
    in_clip: bool,
    refs: &mut Vec<(&'a str, RefKind)>,
) -> Result<(), SvgError> {
    if v.contains(['\\', '@', '{', '}', '!']) || v.contains("/*") || v.contains("*/") {
        return Err(SvgError::Unsupported);
    }
    let mut count = 0;
    for chunk in v.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        count += 1;
        if count > 32 {
            return Err(SvgError::Capacity);
        }
        let (name, value) = chunk.split_once(':').ok_or(SvgError::InvalidInput)?;
        if value.contains(':') {
            return Err(SvgError::Unsupported);
        }
        let mut parser = simplecss::DeclarationTokenizer::from(chunk);
        let dec = parser.next().ok_or(SvgError::InvalidInput)?;
        if dec.important
            || dec.name != name.trim()
            || dec.value != value.trim()
            || parser.next().is_some()
        {
            return Err(SvgError::InvalidInput);
        }
        if presentation(dec.name, dec.value, in_clip, refs)?.is_none() {
            return Err(SvgError::Unsupported);
        }
    }
    Ok(())
}
fn presentation<'a>(
    name: &str,
    v: &'a str,
    in_clip: bool,
    refs: &mut Vec<(&'a str, RefKind)>,
) -> Result<Option<()>, SvgError> {
    match name {
        "fill" | "stroke" => {
            match svgtypes::Paint::from_str(v).map_err(|_| SvgError::InvalidInput)? {
                svgtypes::Paint::FuncIRI(id, _) => {
                    if !ascii_id(id) || in_clip {
                        return Err(SvgError::Unsupported);
                    }
                    refs.push((id, RefKind::Paint));
                }
                svgtypes::Paint::ContextFill | svgtypes::Paint::ContextStroke => {
                    return Err(SvgError::Unsupported);
                }
                _ => {}
            }
        }
        "color" | "stop-color" => {
            if v.trim() != "currentColor" && v.trim() != "inherit" {
                svgtypes::Color::from_str(v).map_err(|_| SvgError::InvalidInput)?;
            }
        }
        "opacity" | "fill-opacity" | "stroke-opacity" | "stop-opacity" => {
            if v.trim() != "inherit" && !(0. ..=1.).contains(&number(v)?) {
                return Err(SvgError::InvalidInput);
            }
        }
        "fill-rule" | "clip-rule" => choice(v, &["nonzero", "evenodd", "inherit"])?,
        "stroke-width" => {
            if v.trim() != "inherit" && !(0. ..=256.).contains(&length(v, false)?) {
                return Err(SvgError::InvalidInput);
            }
        }
        "stroke-miterlimit" => {
            if v.trim() != "inherit" && !(1. ..=16.).contains(&number(v)?) {
                return Err(SvgError::InvalidInput);
            }
        }
        "stroke-linecap" => choice(v, &["butt", "round", "square", "inherit"])?,
        "stroke-linejoin" => choice(v, &["miter", "round", "bevel", "inherit"])?,
        "stroke-dasharray" => {
            if !matches!(v.trim(), "none" | "inherit") {
                let list = number_list(v)?;
                if list.len() > 256 {
                    return Err(SvgError::Capacity);
                }
                if list.is_empty()
                    || list.iter().any(|n| !(0. ..=256.).contains(n))
                    || list.iter().sum::<f64>() == 0.
                {
                    return Err(SvgError::InvalidInput);
                }
            }
        }
        "stroke-dashoffset" => {
            if v.trim() != "inherit" && length(v, false)?.abs() > 256. {
                return Err(SvgError::InvalidInput);
            }
        }
        "display" => choice(v, &["inline", "none", "inherit"])?,
        "visibility" => choice(v, &["visible", "hidden", "inherit"])?,
        "transform" => transform(v)?,
        "clip-path" => {
            if in_clip {
                return Err(SvgError::Unsupported);
            }
            if v.trim() != "none" {
                let id = svgtypes::FuncIRI::from_str(v)
                    .map_err(|_| SvgError::Unsupported)?
                    .0;
                if !ascii_id(id) {
                    return Err(SvgError::InvalidInput);
                }
                refs.push((id, RefKind::Clip));
            }
        }
        _ => return Ok(None),
    };
    Ok(Some(()))
}
fn number_list(v: &str) -> Result<Vec<f64>, SvgError> {
    let mut list = Vec::new();
    for n in svgtypes::NumberListParser::from(v) {
        let n = n.map_err(|_| SvgError::InvalidInput)?;
        coord(n)?;
        list.push(n);
        if list.len() > 8192 {
            return Err(SvgError::Capacity);
        }
    }
    Ok(list)
}
fn transform(v: &str) -> Result<(), SvgError> {
    use svgtypes::TransformListToken::*;
    let mut count = 0;
    for token in svgtypes::TransformListParser::from(v) {
        count += 1;
        if count > 256 {
            return Err(SvgError::Capacity);
        }
        let values: &[f64] = match token.map_err(|_| SvgError::InvalidInput)? {
            Matrix { a, b, c, d, e, f } => &[a, b, c, d, e, f],
            Translate { tx, ty } => &[tx, ty],
            Scale { sx, sy } => &[sx, sy],
            Rotate { angle } | SkewX { angle } | SkewY { angle } => &[angle],
        };
        for n in values {
            coord(*n)?;
        }
    }
    let t = svgtypes::Transform::from_str(v).map_err(|_| SvgError::InvalidInput)?;
    for n in [t.a, t.b, t.c, t.d, t.e, t.f] {
        coord(n)?;
    }
    Ok(())
}
fn path(v: &str, budget: &mut Budget) -> Result<(), SvgError> {
    use svgtypes::PathSegment::*;
    let mut count = 0;
    let mut first = true;
    for s in svgtypes::PathParser::from(v) {
        let s = s.map_err(|_| SvgError::InvalidInput)?;
        if first && !matches!(s, MoveTo { .. }) {
            return Err(SvgError::InvalidInput);
        }
        first = false;
        let (values, radii): (&[f64], bool) = match s {
            MoveTo { x, y, .. } | LineTo { x, y, .. } | SmoothQuadratic { x, y, .. } => {
                (&[x, y], false)
            }
            HorizontalLineTo { x, .. } => (&[x], false),
            VerticalLineTo { y, .. } => (&[y], false),
            CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
                ..
            } => (&[x1, y1, x2, y2, x, y], false),
            SmoothCurveTo { x2, y2, x, y, .. } => (&[x2, y2, x, y], false),
            Quadratic { x1, y1, x, y, .. } => (&[x1, y1, x, y], false),
            EllipticalArc {
                rx,
                ry,
                x_axis_rotation,
                x,
                y,
                ..
            } => (&[rx, ry, x_axis_rotation, x, y], true),
            ClosePath { .. } => (&[], false),
        };
        for n in values {
            coord(*n)?;
        }
        if radii && (values[0] < 0. || values[1] < 0.) {
            return Err(SvgError::InvalidInput);
        }
        count += 1;
        budget.segments += 1;
        if count > 4096 || budget.segments > 65536 {
            return Err(SvgError::Capacity);
        }
    }
    if first {
        return Err(SvgError::InvalidInput);
    }
    Ok(())
}
fn normalized(tree: &usvg::Tree, outer: tiny_skia::Transform) -> Result<(), SvgError> {
    if !tree.filters().is_empty() || !tree.masks().is_empty() || !tree.patterns().is_empty() {
        return Err(SvgError::Unsupported);
    }
    let mut count = 0;
    let mut segments = 0;
    group(tree.root(), outer, 1, &mut count, &mut segments)
}
fn group(
    g: &usvg::Group,
    outer: tiny_skia::Transform,
    depth: usize,
    count: &mut usize,
    segments: &mut usize,
) -> Result<(), SvgError> {
    if depth > 32 {
        return Err(SvgError::Capacity);
    }
    if g.mask().is_some() || !g.filters().is_empty() {
        return Err(SvgError::Unsupported);
    }
    for node in g.children() {
        *count += 1;
        if *count > 8192 {
            return Err(SvgError::Capacity);
        }
        match node {
            usvg::Node::Group(g) => group(g, outer, depth + 1, count, segments)?,
            usvg::Node::Path(p) => {
                let t = outer.pre_concat(p.abs_transform());
                for n in [t.sx, t.kx, t.ky, t.sy, t.tx, t.ty] {
                    coord(f64::from(n))?;
                }
                let n = p.data().segments().count();
                if n > 4096 {
                    return Err(SvgError::Capacity);
                }
                *segments += n;
                if *segments > 65536 {
                    return Err(SvgError::Capacity);
                }
                for point in p.data().points() {
                    let mut point = *point;
                    t.map_point(&mut point);
                    coord(f64::from(point.x))?;
                    coord(f64::from(point.y))?;
                }
            }
            _ => return Err(SvgError::Unsupported),
        }
        let mut result = Ok(());
        node.subroots(|sub| {
            if result.is_ok() {
                result = group(sub, outer, depth + 1, count, segments);
            }
        });
        result?;
    }
    Ok(())
}
