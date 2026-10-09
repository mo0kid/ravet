#![cfg(target_has_atomic = "64")]
use ravet_foundation::{PublishError, SnapshotMailbox};
use std::sync::{Arc, Barrier};

#[test]
fn a_stale_audio_reflection_cannot_replace_a_new_control_restore() {
    let mailbox = SnapshotMailbox::new([0, 0, 0]);
    let audio = mailbox.try_read().unwrap();
    assert_eq!(mailbox.try_publish([42, 42, 42]), Ok(2));
    assert_eq!(
        mailbox.try_publish_if(audio.generation, [7, 7, 7]),
        Err(PublishError::Stale)
    );
    let latest = mailbox.try_read().unwrap();
    assert_eq!(latest.words, [42, 42, 42]);
    assert_eq!(mailbox.try_publish_if(latest.generation, [8, 8, 8]), Ok(4));
}

#[test]
fn concurrent_publications_never_produce_a_torn_snapshot() {
    let mailbox = Arc::new(SnapshotMailbox::new([0; 32]));
    let start = Arc::new(Barrier::new(3));
    let writers: Vec<_> = (1..=2)
        .map(|writer| {
            let mailbox = mailbox.clone();
            let start = start.clone();
            std::thread::spawn(move || {
                start.wait();
                for iteration in 1..=10000 {
                    let word = writer * 100000 + iteration;
                    loop {
                        if mailbox.try_publish([word; 32]).is_ok() {
                            break;
                        }
                        std::thread::yield_now();
                    }
                }
            })
        })
        .collect();
    start.wait();
    for _ in 0..20000 {
        if let Some(snapshot) = mailbox.try_read() {
            assert!(snapshot.words.iter().all(|&word| word == snapshot.words[0]));
            assert_eq!(snapshot.generation & 1, 0);
        }
    }
    for writer in writers {
        writer.join().unwrap();
    }
    assert_eq!(mailbox.try_read().unwrap().generation, 40000);
}
