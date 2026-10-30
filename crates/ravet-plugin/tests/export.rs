use ravet_plugin::{
    Category, Edit, EditQueue, Parameter, ParameterCache, Product, StateFormat, editor,
};
use std::sync::Arc;

const PARAMETERS: &[Parameter] = &[
    Parameter::linear("a", "A", 0.0, 1.0, 0.25),
    Parameter::toggle("b", "B", true),
];
const PRODUCT: Product = Product::new(
    "Test",
    "Test",
    "1.0.0",
    "com.example.test",
    Category::Effect,
    StateFormat {
        tag: *b"TESTEXP1",
        version: 1,
    },
)
.with_parameters(PARAMETERS);

#[test]
fn edit_queue_pushes_all_or_nothing_and_wraps() {
    let queue = EditQueue::new();
    assert!(queue.is_empty());
    assert_eq!(queue.peek(), None);
    for round in 0..3 {
        let full: Vec<_> = (0..EditQueue::CAPACITY)
            .map(|i| Edit::Value(i % 7, i as f32))
            .collect();
        assert!(queue.push(&full[..EditQueue::CAPACITY - 2]));
        assert!(
            !queue.push(&[Edit::Begin(1), Edit::Value(1, 0.5), Edit::End(1)]),
            "round {round}"
        );
        assert!(queue.push(&[Edit::Begin(1), Edit::End(1)]));
        assert!(!queue.push(&[Edit::End(2)]));
        for expected in &full[..EditQueue::CAPACITY - 2] {
            assert_eq!(queue.peek(), Some(*expected));
            assert_eq!(queue.peek(), Some(*expected), "peek does not remove");
            queue.pop();
        }
        assert_eq!(queue.peek(), Some(Edit::Begin(1)));
        queue.pop();
        assert_eq!(queue.peek(), Some(Edit::End(1)));
        queue.pop();
        assert!(queue.is_empty());
        queue.pop(); // popping an empty queue is harmless
    }
    assert!(queue.push(&[Edit::Value(1023, -0.0), Edit::Value(3, f32::MAX)]));
    assert_eq!(queue.peek(), Some(Edit::Value(1023, -0.0)));
}

#[test]
fn edit_queue_keeps_order_across_threads() {
    let queue = Arc::new(EditQueue::new());
    let producer = {
        let queue = queue.clone();
        std::thread::spawn(move || {
            for i in 0..20_000u32 {
                let edits = [
                    Edit::Begin(i as usize % 1024),
                    Edit::Value(i as usize % 1024, i as f32),
                    Edit::End(i as usize % 1024),
                ];
                while !queue.push(&edits) {
                    std::thread::yield_now();
                }
            }
        })
    };
    let mut received = 0u32;
    while received < 20_000 * 3 {
        let Some(edit) = queue.peek() else {
            std::thread::yield_now();
            continue;
        };
        let i = received / 3;
        let index = i as usize % 1024;
        let expected = match received % 3 {
            0 => Edit::Begin(index),
            1 => Edit::Value(index, i as f32),
            _ => Edit::End(index),
        };
        assert_eq!(edit, expected);
        queue.pop();
        received += 1;
    }
    producer.join().unwrap();
    assert!(queue.is_empty());
}

#[test]
fn parameter_cache_restores_with_a_new_generation() {
    let product = PRODUCT.resolve().unwrap();
    let cache = ParameterCache::new(&product);
    assert_eq!(cache.len(), 2);
    assert_eq!((cache.get(0), cache.get(1)), (0.25, 1.0));
    let generation = cache.generation();
    cache.set(0, 0.5);
    assert_eq!(cache.generation(), generation, "edits do not resynchronize");
    cache.restore(&[0.75, 0.0]);
    assert_ne!(cache.generation(), generation);
    let mut values = [0.0; 2];
    cache.read_into(&mut values);
    assert_eq!(values, [0.75, 0.0]);
}

#[test]
fn editor_pages_get_the_client_in_their_head() {
    let tag = format!("<script>{}</script>", editor::CLIENT_SCRIPT);
    assert_eq!(
        editor::page("<html><HEAD lang=\"en\"><title>x</title></head></html>"),
        format!("<html><HEAD lang=\"en\">{tag}<title>x</title></head></html>")
    );
    assert_eq!(editor::page("<p>bare</p>"), format!("{tag}<p>bare</p>"));
    assert!(editor::CLIENT_SCRIPT.contains("window.ravetPlugin"));
}
