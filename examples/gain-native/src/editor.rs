//! Public gain reference UI codecs; native format lifecycle remains the adapter's.
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    ptr, slice,
    sync::OnceLock,
};
const TEMPLATE: &str = include_str!("../../gain-vst3/editor/index.html");
static HTML: OnceLock<String> = OnceLock::new();
/// # Safety
/// Output pointer is valid writable storage; returned bytes remain library-owned.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ravet_gain_editor_html(length: *mut usize) -> *const u8 {
    let Some(length) = (unsafe { length.as_mut() }) else {
        return ptr::null();
    };
    *length = 0;
    catch_unwind(AssertUnwindSafe(|| {
        let html = HTML.get_or_init(|| {
            ravet_surface::embed_web_controls(TEMPLATE).expect("trusted gain editor marker")
        });
        *length = html.len();
        html.as_ptr()
    }))
    .unwrap_or(ptr::null())
}
fn decode(json: &[u8]) -> Option<(u32, u32, f64)> {
    let value: serde_json::Value = serde_json::from_slice(json).ok()?;
    let fields = value.as_object()?;
    if fields.len() != 3 {
        return None;
    }
    let kind = match fields.get("type")?.as_str()? {
        "edit" => 0,
        "begin" => 1,
        "value" => 2,
        "end" => 3,
        _ => return None,
    };
    let id = u32::try_from(fields.get("id")?.as_u64()?).ok()?;
    let physical = fields.get("value")?.as_f64()?;
    if !physical.is_finite() {
        return None;
    }
    let schema = ravet_gain::parameter_schema();
    let descriptor = &schema.descriptors()[schema.index(ravet_foundation::ParameterId(id))?];
    let physical = physical.clamp(
        f64::from(descriptor.range.minimum()),
        f64::from(descriptor.range.maximum()),
    ) as f32;
    Some((kind, id, f64::from(descriptor.range.normalize(physical)?)))
}
/// # Safety
/// Readable input and aligned writable distinct outputs satisfy RavetGainEditor.h.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ravet_gain_editor_message(
    json: *const u8,
    length: usize,
    kind: *mut u32,
    id: *mut u32,
    normalized: *mut f64,
) -> i32 {
    if json.is_null()
        || length == 0
        || length > 65536
        || kind.is_null()
        || id.is_null()
        || normalized.is_null()
    {
        return 1;
    }
    catch_unwind(AssertUnwindSafe(|| {
        let Some((k, i, n)) = decode(unsafe { slice::from_raw_parts(json, length) }) else {
            return 1;
        };
        unsafe {
            kind.write(k);
            id.write(i);
            normalized.write(n);
        }
        0
    }))
    .unwrap_or(3)
}
/// # Safety
/// Output is live writable capacity bytes; no input/output overlaps or DSP use.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ravet_gain_editor_snapshot(
    gain: f64,
    bypass: f64,
    status: u32,
    output: *mut u8,
    capacity: usize,
) -> usize {
    if output.is_null()
        || capacity == 0
        || capacity > 65536
        || !gain.is_finite()
        || !(0.0..=1.).contains(&gain)
        || !bypass.is_finite()
        || !(0.0..=1.).contains(&bypass)
        || status > 2
    {
        return 0;
    }
    catch_unwind(AssertUnwindSafe(|| {
        let status = match status {
            0 => "Ready",
            1 => "Edit rejected",
            _ => "Gesture end rejected",
        };
        let bytes = serde_json::to_vec(
            &serde_json::json!({"gain":gain*4.,"bypass":bypass>=0.5,"status":status}),
        )
        .ok()?;
        if bytes.len() > capacity {
            return None;
        }
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), output, bytes.len());
        }
        Some(bytes.len())
    }))
    .ok()
    .flatten()
    .unwrap_or(0)
}
/// Encode UI-only meter data received through host parameter delivery. Values
/// are twelve normalized peak/RMS/diagnostic triples, input L/R then output L/R.
/// Levels map to 0..4 and diagnostic flags snap at 0.5.
/// # Safety
/// Values point to twelve live readable doubles; output is live writable
/// capacity bytes, distinct from the values. UI thread only; no live DSP access.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ravet_gain_editor_meter_snapshot(
    gain: f64,
    bypass: f64,
    status: u32,
    values: *const f64,
    output: *mut u8,
    capacity: usize,
) -> usize {
    if values.is_null()
        || output.is_null()
        || capacity == 0
        || capacity > 65536
        || !gain.is_finite()
        || !(0. ..=1.).contains(&gain)
        || !bypass.is_finite()
        || !(0. ..=1.).contains(&bypass)
        || status > 2
    {
        return 0;
    }
    catch_unwind(AssertUnwindSafe(|| {
        let values = unsafe { slice::from_raw_parts(values, 12) };
        if values.iter().any(|v| !v.is_finite() || !(0. ..=1.).contains(v)) { return None; }
        let meters: [serde_json::Value; 4] = std::array::from_fn(|index| {
            let start = index * 3;
            serde_json::json!({"peak":values[start]*4.,"rms":values[start+1]*4.,"nonfinite":values[start+2]>=0.5})
        });
        let status = match status {0=>"Ready",1=>"Edit rejected",_=>"Gesture end rejected"};
        let bytes = serde_json::to_vec(&serde_json::json!({"gain":gain*4.,"bypass":bypass>=0.5,"status":status,"meters":meters})).ok()?;
        if bytes.len() > capacity {return None;}
        unsafe {ptr::copy_nonoverlapping(bytes.as_ptr(),output,bytes.len());}
        Some(bytes.len())
    })).ok().flatten().unwrap_or(0)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn host_meter_json_scales_flags_and_rejects_without_mutation() {
        let mut values = [0.; 12];
        values[0] = 0.25;
        values[1] = 0.125;
        values[2] = 1.;
        values[9] = 1.;
        values[10] = 0.5;
        let mut output = [0x55; 1024];
        let count = unsafe {
            ravet_gain_editor_meter_snapshot(
                0.25,
                0.,
                0,
                values.as_ptr(),
                output.as_mut_ptr(),
                output.len(),
            )
        };
        assert!(count > 0);
        let json: serde_json::Value = serde_json::from_slice(&output[..count]).unwrap();
        assert_eq!(json["gain"], 1.);
        assert_eq!(json["meters"][0]["peak"], 1.);
        assert_eq!(json["meters"][0]["rms"], 0.5);
        assert_eq!(json["meters"][0]["nonfinite"], true);
        assert_eq!(json["meters"][3]["peak"], 4.);
        assert_eq!(json["meters"][3]["rms"], 2.);
        for invalid in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
            values[0] = invalid;
            output.fill(0x55);
            assert_eq!(
                unsafe {
                    ravet_gain_editor_meter_snapshot(
                        0.25,
                        0.,
                        0,
                        values.as_ptr(),
                        output.as_mut_ptr(),
                        output.len(),
                    )
                },
                0
            );
            assert_eq!(output, [0x55; 1024]);
        }
        values[0] = 0.;
        assert_eq!(
            unsafe {
                ravet_gain_editor_meter_snapshot(
                    0.25,
                    0.,
                    0,
                    values.as_ptr(),
                    output.as_mut_ptr(),
                    1,
                )
            },
            0
        );
        assert_eq!(output, [0x55; 1024]);
        assert_eq!(decode(br#"{"type":"edit","id":10,"value":1}"#), None);
    }
    #[test]
    fn gain_edit_schema_commands_normalization_and_rejected_outputs() {
        for (text, expected) in [
            (r#"{"type":"edit","id":1,"value":3}"#, (0, 1, 0.75)),
            (r#"{"type":"value","id":1,"value":9}"#, (2, 1, 1.)),
            (r#"{"type":"edit","id":2,"value":0.5}"#, (0, 2, 1.)),
            (r#"{"type":"end","id":1,"value":0}"#, (3, 1, 0.)),
        ] {
            assert_eq!(decode(text.as_bytes()), Some(expected));
        }
        for text in [
            "null",
            r#"{"type":"action","id":1,"value":0}"#,
            r#"{"type":"edit","id":99,"value":0}"#,
            r#"{"type":"edit","id":1,"value":1,"extra":0}"#,
            r#"{"type":"edit","id":1,"value":null}"#,
            r#"{"type":"edit","id":1.5,"value":0}"#,
        ] {
            let (mut kind, mut id, mut value) = (99, 99, 99.);
            assert_eq!(
                unsafe {
                    ravet_gain_editor_message(
                        text.as_ptr(),
                        text.len(),
                        &mut kind,
                        &mut id,
                        &mut value,
                    )
                },
                1
            );
            assert_eq!((kind, id, value), (99, 99, 99.));
        }
    }
    #[test]
    fn snapshot_json_status_bounds_and_compiled_html_are_valid() {
        let mut bytes = [0xa5; 256];
        let count =
            unsafe { ravet_gain_editor_snapshot(0.75, 1., 1, bytes.as_mut_ptr(), bytes.len()) };
        let value: serde_json::Value = serde_json::from_slice(&bytes[..count]).unwrap();
        assert_eq!(value["gain"], 3.);
        assert_eq!(value["bypass"], true);
        assert_eq!(value["status"], "Edit rejected");
        assert!(bytes[count..].iter().all(|b| *b == 0xa5));
        for (gain, bypass, status, capacity) in [
            (f64::NAN, 0., 0, 256),
            (1.1, 0., 0, 256),
            (0., f64::INFINITY, 0, 256),
            (0., 0., 3, 256),
            (0., 0., 0, 1),
        ] {
            let before = bytes;
            assert_eq!(
                unsafe {
                    ravet_gain_editor_snapshot(gain, bypass, status, bytes.as_mut_ptr(), capacity)
                },
                0
            );
            assert_eq!(bytes, before);
        }
        let mut length = 0;
        let html = unsafe { ravet_gain_editor_html(&mut length) };
        let text = std::str::from_utf8(unsafe { slice::from_raw_parts(html, length) }).unwrap();
        assert!(!text.contains("<!-- RAVET_CONTROLS -->"));
        assert!(text.contains("RavetControls.Slider"));
    }
}
