#![cfg(feature = "svg-image")]
use ravet_surface::svg::*;
fn source(body: &str) -> String {
    format!("<svg xmlns='http://www.w3.org/2000/svg' width='16' height='8'>{body}</svg>")
}
fn image(body: &str) -> SvgImage {
    SvgDocument::parse(source(body).as_bytes())
        .unwrap()
        .render(16, 8, SvgFit::Contain)
        .unwrap()
}
fn pixel(im: &SvgImage, x: usize, y: usize) -> &[u8] {
    let p = (y * im.width() as usize + x) * 4;
    &im.pixels()[p..p + 4]
}
fn error(s: &str, e: SvgError) {
    assert!(
        matches!(SvgDocument::parse(s.as_bytes()),Err(actual) if actual==e),
        "{s}"
    );
}
#[test]
fn exact_pixels_ownership_and_fit() {
    let mut s = source("<rect x='2' y='1' width='6' height='4' fill='#c86432'/>");
    let doc = SvgDocument::parse(s.as_bytes()).unwrap();
    s.clear();
    assert_eq!(doc.intrinsic_size(), (16., 8.));
    let im = doc.render(16, 8, SvgFit::Contain).unwrap();
    assert_eq!(pixel(&im, 3, 2), [200, 100, 50, 255]);
    assert_eq!(pixel(&im, 0, 0), [0, 0, 0, 0]);
    let prior = im.clone();
    let fitted = doc.render(16, 16, SvgFit::Contain).unwrap();
    assert_eq!(pixel(&fitted, 3, 6), [200, 100, 50, 255]);
    assert_eq!(pixel(&fitted, 3, 2), [0, 0, 0, 0]);
    let stretched = doc.render(16, 16, SvgFit::Stretch).unwrap();
    assert_eq!(pixel(&stretched, 3, 3), [200, 100, 50, 255]);
    assert_eq!(im, prior);
    assert_eq!(
        doc.render(0, 8, SvgFit::Contain),
        Err(SvgError::InvalidInput)
    );
    assert_eq!(
        doc.render(4097, 8, SvgFit::Contain),
        Err(SvgError::Capacity)
    );
    assert_eq!(im, prior);
}
#[test]
fn root_viewbox_and_clip() {
    let s = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='-2 -1 16 8'><defs><clipPath id='c'><rect x='0' y='0' width='4' height='4'/></clipPath></defs><rect x='-2' y='-1' width='16' height='8' fill='red' clip-path='url(#c)'/></svg>";
    let d = SvgDocument::parse(s.as_bytes()).unwrap();
    assert_eq!(d.intrinsic_size(), (16., 8.));
    let im = d.render(16, 8, SvgFit::Contain).unwrap();
    assert_eq!(pixel(&im, 3, 2), [255, 0, 0, 255]);
    assert_eq!(pixel(&im, 0, 0), [0, 0, 0, 0]);
    assert_eq!(pixel(&im, 8, 2), [0, 0, 0, 0]);
}
#[test]
fn style_inheritance_opacity_and_currentcolor() {
    let im = image(
        "<g color='#008000' fill='currentColor' opacity='.5'><rect width='16' height='8' fill='red' style='fill: currentColor; stroke: none'/></g>",
    );
    assert_eq!(pixel(&im, 4, 4), [0, 64, 0, 128]);
    let im = image(
        "<g transform='translate(2,1) scale(2)' fill='blue'><rect width='2' height='2'/></g>",
    );
    assert_eq!(pixel(&im, 3, 2), [0, 0, 255, 255]);
    assert_eq!(pixel(&im, 1, 2), [0, 0, 0, 0]);
}
#[test]
fn gradients_both_profiles_and_spread() {
    for (tag, attrs) in [
        ("linearGradient", "x1='0%' y1='0%' x2='100%' y2='0%'"),
        (
            "radialGradient",
            "cx='50%' cy='50%' r='50%' spreadMethod='reflect'",
        ),
    ] {
        let im = image(&format!(
            "<defs><{tag} id='p' {attrs}><stop offset='0%' stop-color='red'/><stop offset='100%' stop-color='blue'/></{tag}></defs><rect width='16' height='8' fill='url(#p)'/>"
        ));
        assert_eq!(pixel(&im, 8, 4)[3], 255);
        assert!(pixel(&im, 8, 4)[0] > 0 && pixel(&im, 8, 4)[2] > 0);
    }
}
#[test]
fn all_path_families_and_stroke_dash() {
    let im = image(
        "<path d='M1 1 h4 v2 H4 V4 q1 1 2 0 t2 0 C10 3 12 3 12 4 s2 2 2 0 a2 2 0 0 1 -2 2 L1 6 z' fill='none' stroke='red' stroke-width='1' stroke-linecap='round' stroke-linejoin='bevel' stroke-dasharray='2 1'/> <polygon points='0,0 1,0 0,1' fill='blue'/>",
    );
    assert!(im.pixels().as_chunks::<4>().0.iter().any(|p| p[3] > 0));
}
#[test]
fn resource_and_unknown_refusal() {
    for b in [
        "<image href='/tmp/secret.png'/>",
        "<image href='data:image/svg+xml;base64,PHN2Zy8+'/>",
        "<use href='#a'/>",
        "<text>Hi</text>",
        "<style>@import url('x');</style>",
        "<script/>",
        "<foreignObject/>",
        "<filter/>",
        "<mask/>",
        "<pattern/>",
        "<g onclick='x()'/>",
        "<rect fill='url(https://x/a)' width='1' height='1'/>",
        "<svg/>",
    ] {
        assert!(SvgDocument::parse(source(b).as_bytes()).is_err(), "{b}");
    }
    error(
        "<!DOCTYPE svg><svg xmlns='http://www.w3.org/2000/svg' width='1' height='1'/>",
        SvgError::Unsupported,
    );
    error(
        "<?xml-stylesheet href='x'?><svg xmlns='http://www.w3.org/2000/svg' width='1' height='1'/>",
        SvgError::Unsupported,
    );
    error(
        &source("<g xmlns:x='http://www.w3.org/1999/xlink' x:href='/tmp/a'/>"),
        SvgError::Unsupported,
    );
    error(
        &source("<rect style='fill: url(&#104;ttps://x)'/>"),
        SvgError::Unsupported,
    );
    error(
        &source("<rect style='fill: red; broken'/>"),
        SvgError::InvalidInput,
    );
    error(
        &source("<rect style='fill:red !important'/>"),
        SvgError::Unsupported,
    );
    error(
        &source("<rect style='fi\\6cl:red'/>"),
        SvgError::Unsupported,
    );
}
#[test]
fn reference_target_and_clip_cycles() {
    for body in [
        "<rect width='1' height='1' fill='url(#missing)'/>",
        "<g id='x'/><rect fill='url(#x)'/>",
        "<linearGradient id='x'/><rect clip-path='url(#x)'/>",
        "<g id='x'/><g id='x'/>",
        "<clipPath id='x'><rect clip-path='url(#x)'/></clipPath>",
        "<linearGradient href='#x'/>",
    ] {
        assert!(
            SvgDocument::parse(source(body).as_bytes()).is_err(),
            "{body}"
        );
    }
}
#[test]
fn invalid_syntax_and_numeric_values() {
    for body in [
        "<path d='M0 0 L NaN 0'/>",
        "<path d='L1 1'/>",
        "<path d='M0 0 A-1 1 0 0 0 2 2'/>",
        "<rect width='1cm'/>",
        "<rect width='10%'/>",
        "<g opacity='2'/>",
        "<g transform='scale(1e300) scale(1e-300)'/>",
        "<polyline points='0,0 1,1 bad'/>",
        "<polygon points='0 0 1'/>",
        "<g transform='translate(65536)'><rect x='2' width='1' height='1'/></g>",
    ] {
        assert!(
            SvgDocument::parse(source(body).as_bytes()).is_err(),
            "{body}"
        );
    }
    for s in [
        "<svg xmlns='http://www.w3.org/2000/svg'/>",
        "<svg xmlns='http://www.w3.org/2000/svg' width='1' height='0'/>",
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 0 1'/>",
    ] {
        assert!(SvgDocument::parse(s.as_bytes()).is_err());
    }
}
#[test]
fn source_element_depth_and_id_boundaries() {
    assert!(matches!(
        SvgDocument::parse(&vec![b' '; 1024 * 1024 + 1]),
        Err(SvgError::Capacity)
    ));
    assert!(SvgDocument::parse(source(&"<g/>".repeat(4095)).as_bytes()).is_ok());
    error(&source(&"<g/>".repeat(4096)), SvgError::Capacity);
    assert!(
        SvgDocument::parse(
            source(&format!("{}{}", "<g>".repeat(31), "</g>".repeat(31))).as_bytes()
        )
        .is_ok()
    );
    error(
        &source(&format!("{}{}", "<g>".repeat(32), "</g>".repeat(32))),
        SvgError::Capacity,
    );
    let a = (0..1024)
        .map(|i| format!("<g id='a{i}'/>"))
        .collect::<String>();
    assert!(SvgDocument::parse(source(&a).as_bytes()).is_ok());
    error(&source(&(a + "<g id='extra'/>")), SvgError::Capacity);
}
#[test]
fn path_and_normalized_reference_budget() {
    let p = format!("M0 0 {}", "L1 1 ".repeat(4095));
    assert!(SvgDocument::parse(source(&format!("<path d='{p}'/>")).as_bytes()).is_ok());
    error(
        &source(&format!("<path d='{p} L1 1'/>")),
        SvgError::Capacity,
    );
    let body = format!(
        "<defs><clipPath id='c'><path d='{p}'/></clipPath></defs>{}",
        "<rect width='1' height='1' clip-path='url(#c)'/>".repeat(17)
    );
    assert!(matches!(
        SvgDocument::parse(source(&body).as_bytes()),
        Err(SvgError::Capacity)
    ));
}
#[test]
fn whole_first_party_logo() {
    let d = SvgDocument::parse(include_bytes!("../../../media/ravet_LOGO.svg")).unwrap();
    assert_eq!(d.intrinsic_size(), (1119., 350.));
    let im = d.render(320, 100, SvgFit::Contain).unwrap();
    assert!(
        im.pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[3] > 0)
            .count()
            > 3000
    );
}
#[test]
fn exact_source_node_attribute_and_style_limits() {
    let prefix = "<svg xmlns='http://www.w3.org/2000/svg' width='1' height='1'><!--";
    let suffix = "--></svg>";
    let s = format!(
        "{prefix}{}{suffix}",
        "x".repeat(1024 * 1024 - prefix.len() - suffix.len())
    );
    assert_eq!(s.len(), 1024 * 1024);
    assert!(SvgDocument::parse(s.as_bytes()).is_ok());
    assert!(SvgDocument::parse(source(&"<!--x-->".repeat(8190)).as_bytes()).is_ok());
    error(&source(&"<!--x-->".repeat(8191)), SvgError::Capacity);
    assert!(
        SvgDocument::parse(source(&format!("<g aria-label='{}'/>", "x".repeat(65536))).as_bytes())
            .is_ok()
    );
    error(
        &source(&format!("<g aria-label='{}'/>", "x".repeat(65537))),
        SvgError::Capacity,
    );
    assert!(
        SvgDocument::parse(source(&format!("<g style='{}'/>", "fill: red;".repeat(32))).as_bytes())
            .is_ok()
    );
    error(
        &source(&format!("<g style='{}'/>", "fill:red;".repeat(33))),
        SvgError::Capacity,
    );
}
#[test]
fn exact_output_and_cumulative_path_stop_boundaries() {
    let d = SvgDocument::parse(source("").as_bytes()).unwrap();
    let im = d.render(4096, 4096, SvgFit::Contain).unwrap();
    assert_eq!(im.pixels().len(), 64 * 1024 * 1024);
    assert!(im.pixels().iter().all(|v| *v == 0));
    let p = format!("M0 0 {}", "L1 1 ".repeat(4095));
    let shapes = format!("<path d='{p}'/>").repeat(16);
    assert!(SvgDocument::parse(source(&shapes).as_bytes()).is_ok());
    error(&source(&(shapes + "<path d='M0 0'/>")), SvgError::Capacity);
    let stops = "<stop offset='0' stop-color='red'/>".repeat(256);
    assert!(
        SvgDocument::parse(source(&format!("<linearGradient>{stops}</linearGradient>")).as_bytes())
            .is_ok()
    );
    error(
        &source(&format!(
            "<linearGradient>{stops}<stop offset='1'/></linearGradient>"
        )),
        SvgError::Capacity,
    );
}
