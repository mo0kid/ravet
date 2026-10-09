use ravet_runtime::{
    files::{self, FileError},
    tasks::{PeriodicTask, SerialExecutor, SubmitError},
};
use std::{
    io::{self, Read},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};

#[test]
fn bounded_binary_files_and_failed_replace_preserve_destination() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("nested/résumé.bin");
    files::atomic_write(&path, b"a\0b").unwrap();
    assert_eq!(files::read(&path, 3).unwrap(), b"a\0b");
    assert!(matches!(files::read(&path, 2), Err(FileError::TooLarge)));
    let replacement = vec![5; 20_000];
    files::atomic_write(&path, &replacement).unwrap();
    assert_eq!(files::read(&path, 20_000).unwrap(), replacement);
    files::atomic_write(&path, &[]).unwrap();
    assert!(files::read(&path, 0).unwrap().is_empty());
    assert!(matches!(
        files::read(root.path().join("missing"), 32),
        Err(FileError::Io(_))
    ));
    assert!(files::atomic_write(path.parent().unwrap(), b"bad").is_err());
    assert!(path.parent().unwrap().is_dir());
    assert!(std::fs::read_dir(root.path()).unwrap().all(|e| {
        !e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".ravet-write-")
    }));
}
#[test]
fn bounded_reader_stops_at_one_extra_byte_and_propagates_io_error() {
    let mut infinite = io::repeat(1);
    assert!(matches!(
        files::read_bounded(&mut infinite, 8192),
        Err(FileError::TooLarge)
    ));
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("broken"))
        }
    }
    assert!(matches!(
        files::read_bounded(Broken, 5),
        Err(FileError::Io(_))
    ));
    assert!(matches!(
        files::read_bounded(io::empty(), usize::MAX),
        Err(FileError::InvalidLimit)
    ));
}
#[test]
fn worker_owns_captures_bounds_queue_and_drains_accepted_work() {
    let executor = SerialExecutor::new(1).unwrap();
    let (started, has_started) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    executor
        .submit(move || {
            started.send(()).unwrap();
            gate.recv().unwrap();
        })
        .unwrap();
    has_started.recv_timeout(Duration::from_secs(2)).unwrap();
    let (done, completed) = mpsc::channel();
    {
        let temporary = "x".repeat(4096);
        executor
            .submit(move || done.send(temporary).unwrap())
            .unwrap();
    }
    assert_eq!(
        executor.submit(|| panic!("rejected task must never run")),
        Err(SubmitError::Full)
    );
    release.send(()).unwrap();
    drop(executor);
    assert_eq!(completed.recv().unwrap(), "x".repeat(4096));
}
#[test]
fn worker_survives_panic_and_last_owner_can_drop_inside_task() {
    let queue = Arc::new(SerialExecutor::new(2).unwrap());
    queue.submit(|| panic!("contained worker panic")).unwrap();
    let owner = queue.clone();
    let (done, completed) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    queue
        .submit(move || {
            gate.recv().unwrap();
            drop(owner);
            done.send(()).unwrap();
        })
        .unwrap();
    drop(queue);
    release.send(()).unwrap();
    completed.recv_timeout(Duration::from_secs(2)).unwrap();
}
#[test]
fn stopping_timer_prevents_further_callbacks() {
    let ticks = Arc::new(AtomicUsize::new(0));
    let count = ticks.clone();
    let (first, received) = mpsc::channel();
    let mut timer = PeriodicTask::start(Duration::from_millis(5), move || {
        if count.fetch_add(1, Ordering::SeqCst) == 0 {
            first.send(()).unwrap();
        }
    })
    .unwrap();
    received.recv_timeout(Duration::from_secs(2)).unwrap();
    timer.stop();
    let stopped = ticks.load(Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(ticks.load(Ordering::SeqCst), stopped);
    timer.stop();
    assert!(PeriodicTask::start(Duration::ZERO, || {}).is_err());
    assert!(SerialExecutor::new(0).is_err());
}
