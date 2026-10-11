use crate::layout::{GridArea, LayoutError, LayoutItem, Rect, layout_grid};
const IDS: [&str; 5] = ["header", "library", "editor", "inspector", "status"];
fn track(minimum: f64, preferred: f64, maximum: f64, grow: f64) -> LayoutItem {
    LayoutItem {
        minimum,
        preferred,
        maximum,
        grow,
    }
}
fn area(row: usize, column: usize, row_span: usize, column_span: usize) -> GridArea {
    GridArea {
        row,
        column,
        row_span,
        column_span,
    }
}
pub fn geometry(width: f64, height: f64) -> Result<(&'static str, [Rect; 5]), LayoutError> {
    let bounds = Rect::new(0., 0., width, height).ok_or(LayoutError::InvalidInput)?;
    let inner = bounds.inset(12.).ok_or(LayoutError::InsufficientSpace)?;
    let mut output = [inner; 5];
    let mode = if width >= 800. {
        layout_grid(
            inner,
            &[
                track(140., 180., 280., 1.),
                track(220., 360., 4096., 3.),
                track(140., 180., 280., 1.),
            ],
            &[
                LayoutItem::fixed(48.),
                track(180., 400., 4096., 1.),
                LayoutItem::fixed(28.),
            ],
            8.,
            8.,
            &[
                area(0, 0, 1, 3),
                area(1, 0, 1, 1),
                area(1, 1, 1, 1),
                area(1, 2, 1, 1),
                area(2, 0, 1, 3),
            ],
            &mut output,
        )?;
        "three columns"
    } else {
        layout_grid(
            inner,
            &[track(120., 160., 220., 1.), track(220., 360., 4096., 3.)],
            &[
                LayoutItem::fixed(48.),
                track(180., 300., 4096., 1.),
                LayoutItem::fixed(80.),
                LayoutItem::fixed(28.),
            ],
            8.,
            8.,
            &[
                area(0, 0, 1, 2),
                area(1, 0, 1, 1),
                area(1, 1, 1, 1),
                area(2, 0, 1, 2),
                area(3, 0, 1, 2),
            ],
            &mut output,
        )?;
        "two columns"
    };
    Ok((mode, output))
}
/// Fixed numeric fields and constant labels are serialized outside audio. Geometry
/// is finite before serialization; this format contains no untrusted string data.
pub fn state(width: f64, height: f64) -> String {
    match geometry(width, height) {
        Ok((mode, rects)) => {
            let mut json = format!(
                "{{\"kind\":\"layout\",\"width\":{width},\"height\":{height},\"mode\":\"{mode}\",\"io\":{{\"audio\":false,\"input\":false,\"midi\":false}},\"panels\":["
            );
            for (index, (id, r)) in IDS.iter().zip(rects).enumerate() {
                if index > 0 {
                    json.push(',');
                }
                json.push_str(&format!(
                    "{{\"id\":\"{id}\",\"x\":{},\"y\":{},\"width\":{},\"height\":{}}}",
                    r.x(),
                    r.y(),
                    r.width(),
                    r.height()
                ));
            }
            json.push_str("]}");
            json
        }
        Err(error) => format!(
            "{{\"kind\":\"layout-error\",\"error\":\"{error:?}\",\"io\":{{\"audio\":false,\"input\":false,\"midi\":false}}}}"
        ),
    }
}
