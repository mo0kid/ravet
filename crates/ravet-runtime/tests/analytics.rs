use ravet_runtime::analytics::*;
fn event(name: &str) -> AnalyticsEvent {
    AnalyticsEvent::new(name, 1234, &[("frames", 512.), ("seconds", 1.5)]).unwrap()
}
#[test]
fn default_off_revoke_and_empty_flush_never_invoke_sender() {
    let mut queue = AnalyticsQueue::new(4, 4096).unwrap();
    assert!(!queue.is_enabled());
    assert_eq!(
        queue.record(event("opened")).unwrap(),
        RecordOutcome::Disabled
    );
    assert_eq!(queue.pending(), 0);
    assert_eq!(
        queue.flush::<()>(|_| panic!("disabled transmission")),
        Ok(0)
    );
    queue.set_enabled(true);
    queue.record(event("opened")).unwrap();
    assert_eq!(queue.pending(), 1);
    queue.set_enabled(false);
    assert_eq!(queue.pending(), 0);
    assert_eq!(queue.flush::<()>(|_| panic!("revoked transmission")), Ok(0));
    queue.set_enabled(true);
    assert_eq!(queue.flush::<()>(|_| panic!("empty transmission")), Ok(0));
}
#[test]
fn whole_batch_order_json_failed_retry_and_capacity_preserve_accepted_events() {
    let mut queue = AnalyticsQueue::new(2, 4096).unwrap();
    queue.set_enabled(true);
    queue.record(event("first")).unwrap();
    queue.record(event("second")).unwrap();
    assert_eq!(queue.record(event("third")), Err(AnalyticsError::Capacity));
    let mut failed_body = Vec::new();
    assert_eq!(
        queue.flush(|body| {
            failed_body.extend_from_slice(body);
            Err("lost response")
        }),
        Err("lost response")
    );
    assert_eq!(queue.pending(), 2);
    let parsed: serde_json::Value = serde_json::from_slice(&failed_body).unwrap();
    assert_eq!(parsed["events"][0]["name"], "first");
    assert_eq!(parsed["events"][1]["name"], "second");
    assert_eq!(parsed["events"][0]["metrics"]["seconds"], 1.5);
    assert_eq!(parsed["events"][0]["time_ms"], 1234);
    assert_eq!(
        queue.flush::<()>(|body| {
            assert_eq!(body, failed_body);
            Ok(())
        }),
        Ok(2)
    );
    assert_eq!(queue.pending(), 0);
    let mut tiny = AnalyticsQueue::new(1, 13).unwrap();
    tiny.set_enabled(true);
    assert_eq!(tiny.record(event("first")), Err(AnalyticsError::Capacity));
    assert_eq!(tiny.pending(), 0);
}
#[test]
fn invalid_names_metrics_and_limits_reject_without_collection() {
    for name in ["", "space name", "é", "x\n", "../file"] {
        assert!(AnalyticsEvent::new(name, 0, &[]).is_err());
    }
    assert!(AnalyticsEvent::new(&"a".repeat(65), 0, &[]).is_err());
    assert!(AnalyticsEvent::new("event", 0, &[("x", f64::NAN)]).is_err());
    assert!(AnalyticsEvent::new("event", 0, &[("x", 1.), ("x", 2.)]).is_err());
    assert!(AnalyticsEvent::new("event", 0, &[("x", 1.); 65]).is_err());
    let keys: Vec<_> = (0..64).map(|i| format!("{:064}", i)).collect();
    let metrics: Vec<_> = keys.iter().map(|key| (key.as_str(), 1.)).collect();
    assert!(AnalyticsEvent::new("event", 0, &metrics).is_err());
    for (count, bytes) in [(0, 4096), (1025, 4096), (1, 12), (1, 1_048_577)] {
        assert!(AnalyticsQueue::new(count, bytes).is_err());
    }
}
