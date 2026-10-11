#[cfg(all(target_os = "macos", feature = "live-ui"))]
#[test]
fn content_policy_resolver_is_allocation_free_for_free_and_aspect_sweeps() {
    use ravet_webview::ContentConstraints;
    let policy = ContentConstraints::new(640., 480., 1600., 1200.).unwrap();
    let counts = allocation_counter::measure(|| {
        for step in 1..10000 {
            let width = f64::from((step * 97) % 5000 + 1);
            let height = f64::from((step * 71) % 5000 + 1);
            for aspect in [None, Some((4., 3.))] {
                let p = policy.resolve(width, height, aspect).unwrap();
                assert!((640. ..=1600.).contains(&p.0) && (480. ..=1200.).contains(&p.1));
                if aspect.is_some() {
                    assert!((p.0 - p.1 * 4. / 3.).abs() < 1e-10);
                }
            }
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
}
