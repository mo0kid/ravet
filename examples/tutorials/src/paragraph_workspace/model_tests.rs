use super::*;
fn model() -> Model {
    Model::new(
        authored_fonts().unwrap(),
        "Authored local fonts + Noto Arabic",
    )
    .unwrap()
}
#[test]
fn all_controls_prepare_whole_and_original_source_geometry_is_retained() {
    let mut m = model();
    for sample in 0..8 {
        for raster in [false, true] {
            for size in 0..3 {
                for width in [80, 160, 320, 640] {
                    m.change(State {
                        sample,
                        raster,
                        size,
                        width,
                        rtl: sample % 2 == 1,
                        alignment: sample % 3,
                    })
                    .unwrap();
                    assert!(m.snapshot.len() <= 65536);
                    assert!(m.prepared.width <= 704);
                    assert!(m.prepared.height <= 384);
                    assert!(m.prepared.lines >= 1);
                    assert!(m.prepared.runs <= 512);
                    assert!(m.prepared.payload.len() <= 60000);
                }
            }
        }
    }
}
#[test]
fn refused_large_document_preserves_prior_image_source_and_geometry_but_publishes_diagnostic() {
    let mut m = model();
    let payload = m.payload().to_owned();
    let geometry = m.prepared.geometry.clone();
    let source = m.prepared.source.clone();
    let state = m.state();
    assert_eq!(m.change(State { sample: 8, ..state }), Err(Error::Capacity));
    assert_eq!(m.state(), state);
    assert_eq!(m.payload(), payload);
    assert_eq!(m.prepared.geometry, geometry);
    assert_eq!(m.prepared.source, source);
    assert_eq!(m.diagnostic(), Some(Error::Capacity));
    assert_eq!(m.revision(), 1);
    m.change(state).unwrap();
    assert_eq!(m.diagnostic(), None);
    assert_eq!(m.payload(), payload);
    assert_eq!(m.revision(), 2);
}
#[test]
fn exact_command_revision_shape_and_disposal_independent_model_boundaries() {
    let mut m = model();
    let original = m.snapshot.clone();
    for invalid in [
        "{\"sample\":1}",
        "{\"sample\":1,\"sample\":2}",
        "{\"revision\":0,\"revision\":0}",
        "{\"sample\":+1,\"revision\":0}",
        "{\"sample\":01,\"revision\":0}",
        "{\"sample\":1,\"revision\":1}",
        "{\"width\":79,\"revision\":0}",
        "{\"mode\":2,\"revision\":0}",
    ] {
        assert!(!m.message(invalid));
        assert_eq!(m.snapshot, original);
    }
    assert!(m.message("{\"revision\":0,\"sample\":1}"));
    assert_eq!(m.state().sample, 1);
    assert_eq!(m.revision(), 1);
    assert!(!m.message("{\"sample\":0,\"revision\":0}"));
    assert_eq!(m.state().sample, 1);
    m.revision = u32::MAX;
    let prior = m.snapshot.clone();
    assert_eq!(
        m.change(State {
            sample: 0,
            ..m.state()
        }),
        Err(Error::Capacity)
    );
    assert_eq!(m.snapshot, prior);
}
#[test]
fn measured_resize_margin_noop_and_too_small_failure_are_explicit() {
    let mut m = model();
    m.resize(340.75).unwrap();
    assert_eq!(m.state().width, 300);
    let snapshot = m.snapshot.clone();
    m.resize(340.99).unwrap();
    assert_eq!(m.snapshot, snapshot);
    for width in [119., f64::NAN, f64::INFINITY] {
        assert_eq!(m.resize(width), Err(Error::InvalidInput));
        assert_eq!(m.snapshot, snapshot);
    }
    m.resize(800.).unwrap();
    assert_eq!(m.state().width, 640);
    m.resize(120.).unwrap();
    assert_eq!(m.state().width, 80);
}

#[test]
fn refresh_recovers_current_revision_without_edits_or_stale_revision_admission() {
    let mut m = model();
    assert!(m.message("{\"sample\":1,\"revision\":0}"));
    let payload = m.payload().to_owned();
    let state = m.state();
    assert!(!m.message("{\"sample\":0,\"revision\":0}"));
    let cached = m.snapshot.clone();
    assert!(m.message("{\"refresh\":1}"));
    assert_ne!(m.snapshot, cached);
    assert_eq!(m.revision(), 1);
    assert_eq!(m.delivery, 1);
    assert_eq!(m.state(), state);
    assert_eq!(m.payload(), payload);
    assert!(m.message("{\"width\":160,\"revision\":1}"));
    assert_eq!(m.revision(), 2);
    m.delivery = u32::MAX;
    let before = m.snapshot.clone();
    assert!(m.message("{\"refresh\":1}"));
    assert_ne!(m.snapshot, before);
    assert!(m.refresh_exhausted);
    assert_eq!(m.delivery, u32::MAX);
    let terminal = m.snapshot.clone();
    assert!(!m.message("{\"refresh\":1}"));
    assert!(!m.message("{\"width\":320,\"revision\":2}"));
    assert_eq!(m.snapshot, terminal);
    assert_eq!(m.revision(), 2);
}
#[test]
fn actual_envelope_capacity_failure_uses_normal_error_publication() {
    let mut m = model();
    let state = m.state();
    let previous = (
        m.payload().to_owned(),
        m.prepared.geometry.clone(),
        m.prepared.source.clone(),
    );
    let mut p = prepare(&m.fonts, state).unwrap();
    // Individually bounded payload and geometry can jointly exceed the native
    // JSON envelope. This exercises real formatting/capacity, not an injected
    // error. A valid SVG with inert whitespace reaches the payload bound.
    let xml = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\">{}</svg>",
        " ".repeat(44800)
    );
    p.payload = base64(xml.as_bytes());
    assert!(p.payload.len() <= 60000);
    p.runs = 512;
    p.geometry = format!(
        "[{{\"start\":0,\"end\":1,\"separatorEnd\":1,\"width\":1,\"x\":0,\"y\":0,\"overflow\":false,\"runs\":[{}]}}]",
        vec!["[0,0,1,0,0]"; 512].join(",")
    );
    p.lines = 1;
    assert_eq!(
        snapshot(&m.source_name, state, 1, m.delivery, false, &p, None),
        Err(Error::Capacity)
    );
    assert_eq!(
        m.finish_change(
            State {
                width: 160,
                ..state
            },
            1,
            Ok(p)
        ),
        Err(Error::Capacity)
    );
    assert_eq!(m.revision(), 1);
    assert_eq!(m.state(), state);
    assert_eq!(m.diagnostic(), Some(Error::Capacity));
    assert_eq!(
        (
            m.payload().to_owned(),
            m.prepared.geometry.clone(),
            m.prepared.source.clone()
        ),
        previous
    );
    assert!(m.snapshot.contains("\"diagnostic\":\"Capacity\""));
    assert!(m.snapshot.len() <= 65536);
}

#[test]
fn missed_max_delivery_terminal_ack_preserves_current_model_and_refuses_all_edits() {
    let mut m = model();
    m.delivery = u32::MAX - 1;
    assert!(m.message("{\"refresh\":1}"));
    assert_eq!(m.delivery, u32::MAX);
    assert!(!m.refresh_exhausted);
    let previous = (
        m.state(),
        m.revision(),
        m.payload().to_owned(),
        m.prepared.geometry.clone(),
    );
    let lost = m.snapshot.clone();
    assert!(!m.message("{\"width\":160,\"revision\":0}"));
    assert_eq!(
        m.change(State {
            width: 160,
            ..m.state()
        }),
        Err(Error::Capacity)
    );
    assert_eq!(m.snapshot, lost);
    assert!(m.message("{\"refresh\":1}"));
    assert!(m.refresh_exhausted);
    assert_eq!(m.delivery, u32::MAX);
    assert_eq!(
        (
            m.state(),
            m.revision(),
            m.payload().to_owned(),
            m.prepared.geometry.clone()
        ),
        previous
    );
    assert!(m.snapshot.contains("\"refreshExhausted\":true"));
    let acknowledged = m.snapshot.clone();
    for _ in 0..3 {
        assert!(!m.message("{\"refresh\":1}"));
        assert_eq!(m.snapshot, acknowledged);
    }
}
