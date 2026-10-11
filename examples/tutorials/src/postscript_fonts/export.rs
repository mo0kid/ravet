use crate::{first_party::Source, model::*};
pub fn run() {
    let s = Source::new();
    let initial = s.catalog().selection(0).unwrap();
    let mut m = Model::new(s, initial).unwrap();
    let mut snapshots = Vec::new();
    for style in ["CFF1", "CFF2 100", "CFF2 400", "CFF2 650", "CFF2 900"] {
        let selected = m
            .catalog()
            .styles("RAVET authored PostScript")
            .into_iter()
            .find(|s| m.catalog().descriptor(*s).unwrap().style() == style)
            .unwrap();
        m.select(selected).unwrap();
        for sample in 0..2 {
            for size in 0..3 {
                for mode in 0..2 {
                    for (key, value) in [("sample", sample), ("size", size), ("mode", mode)] {
                        assert!(m.message(&format!(
                            "{{\"{key}\":{value},\"revision\":{}}}",
                            m.revision()
                        )));
                    }
                    assert_eq!(m.selected(), selected);
                    assert!(m.payload().len() < 65536 && m.diagnostic().is_none());
                    snapshots.push(m.snapshot.clone());
                }
            }
        }
    }
    println!("[{}]", snapshots.join(","));
}
