//! The RAVET editor protocol: JSON text messages between a plug-in and its web
//! editor, addressed by parameter key.
//!
//! Every format adapter uses this protocol, so products write no editor codec.
//! Messages are JSON objects with a `type` field. Values are physical values in
//! the parameter's range unless sent as `normalized` (0 to 1).
//!
//! **Editor to plug-in** ([`parse`]), at most [`MAXIMUM_INCOMING_BYTES`]:
//!
//! | `type` | Fields | Meaning |
//! |---|---|---|
//! | `ready` | `protocol` | Page loaded; reply with `info` and `snapshot` |
//! | `begin` | `key` | Start a continuous gesture |
//! | `set` | `key`, `value` or `normalized` | A value inside an open gesture |
//! | `end` | `key` | End a gesture |
//! | `commit` | `key`, `value` or `normalized` | One complete edit (begin, set, end) |
//! | `product` | `data` | Product-defined message |
//!
//! **Plug-in to editor** (built by the functions below):
//!
//! | `type` | Fields |
//! |---|---|
//! | `info` | `protocol`, `product` (name, vendor, version), `parameters` (metadata by key, in declaration order) |
//! | `snapshot` | `sequence`, `parameters` (all values by key), optional `telemetry` and `product` |
//! | `changes` | `sequence`, `parameters` (changed values by key) |
//! | `product` | `data` |
//!
//! `sequence` increases with every snapshot or change message, so an editor can
//! ignore stale data. All functions allocate and run on the main thread.
use crate::product::{Category, Parameter, ParameterRole, ResolvedProduct};
use ravet_foundation::ParameterScale;
use serde_json::{Map, Value, json};

/// Protocol version carried by `ready` and `info`.
pub const PROTOCOL_VERSION: u64 = 1;
/// Largest accepted editor message, matching the web view message limit.
pub const MAXIMUM_INCOMING_BYTES: usize = 65_536;

/// A validated editor message. Parameters are identified by declaration index.
#[derive(Clone, Debug, PartialEq)]
pub enum EditorMessage {
    Ready,
    Begin {
        index: usize,
    },
    /// The value is already clamped and snapped to the parameter's range.
    Set {
        index: usize,
        value: f32,
    },
    End {
        index: usize,
    },
    Commit {
        index: usize,
        value: f32,
    },
    Product(Value),
}

/// Reasons an editor message is rejected. Nothing is applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProtocolError {
    TooLarge,
    /// Not a JSON object, or a required field is missing or has the wrong type.
    Malformed,
    UnknownType(String),
    UnsupportedVersion(u64),
    UnknownKey(String),
    /// Read-only parameters cannot be edited.
    ReadOnly(String),
    /// The value is not finite.
    InvalidValue(String),
}
impl core::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "editor protocol error: {self:?}")
    }
}
impl std::error::Error for ProtocolError {}

/// Parse and validate one editor message.
pub fn parse(product: &ResolvedProduct<'_>, text: &str) -> Result<EditorMessage, ProtocolError> {
    if text.len() > MAXIMUM_INCOMING_BYTES {
        return Err(ProtocolError::TooLarge);
    }
    let message: Value = serde_json::from_str(text).map_err(|_| ProtocolError::Malformed)?;
    let object = message.as_object().ok_or(ProtocolError::Malformed)?;
    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or(ProtocolError::Malformed)?;
    match kind {
        "ready" => {
            let version = object
                .get("protocol")
                .and_then(Value::as_u64)
                .ok_or(ProtocolError::Malformed)?;
            if version != PROTOCOL_VERSION {
                return Err(ProtocolError::UnsupportedVersion(version));
            }
            Ok(EditorMessage::Ready)
        }
        "begin" => Ok(EditorMessage::Begin {
            index: editable(product, object)?.0,
        }),
        "end" => Ok(EditorMessage::End {
            index: editable(product, object)?.0,
        }),
        "set" | "commit" => {
            let (index, parameter) = editable(product, object)?;
            let value = value(parameter, object)?;
            Ok(if kind == "set" {
                EditorMessage::Set { index, value }
            } else {
                EditorMessage::Commit { index, value }
            })
        }
        "product" => Ok(EditorMessage::Product(
            object
                .get("data")
                .cloned()
                .ok_or(ProtocolError::Malformed)?,
        )),
        other => Err(ProtocolError::UnknownType(other.into())),
    }
}

fn editable<'p>(
    product: &'p ResolvedProduct<'_>,
    object: &Map<String, Value>,
) -> Result<(usize, &'p Parameter<'p>), ProtocolError> {
    let key = object
        .get("key")
        .and_then(Value::as_str)
        .ok_or(ProtocolError::Malformed)?;
    let index = product
        .parameter_index_by_key(key)
        .ok_or_else(|| ProtocolError::UnknownKey(key.into()))?;
    let parameter = &product.product().parameters()[index];
    if parameter.is_read_only() {
        return Err(ProtocolError::ReadOnly(key.into()));
    }
    Ok((index, parameter))
}

fn value(parameter: &Parameter<'_>, object: &Map<String, Value>) -> Result<f32, ProtocolError> {
    let invalid = || ProtocolError::InvalidValue(parameter.key().into());
    let number = |field: &str| {
        object
            .get(field)
            .map(|v| v.as_f64().ok_or(ProtocolError::Malformed))
    };
    let range = parameter.range();
    match (number("value"), number("normalized")) {
        (Some(physical), None) => range.canonicalize(physical? as f32).ok_or_else(invalid),
        (None, Some(normalized)) => range.denormalize(normalized? as f32).ok_or_else(invalid),
        _ => Err(ProtocolError::Malformed),
    }
}

/// Tracks open gestures for one editor, so adapters can reject unbalanced
/// messages and end every gesture when the editor closes.
#[derive(Clone, Debug)]
pub struct Gestures {
    open: Vec<bool>,
}

/// A gesture message that does not match the open gestures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GestureError {
    AlreadyOpen(usize),
    NotOpen(usize),
}

impl Gestures {
    pub fn new(product: &ResolvedProduct<'_>) -> Self {
        Self {
            open: vec![false; product.parameter_ids().len()],
        }
    }
    /// Check a parsed message against the open gestures and update them.
    /// `commit` is rejected while a gesture on the same parameter is open.
    pub fn apply(&mut self, message: &EditorMessage) -> Result<(), GestureError> {
        match *message {
            EditorMessage::Begin { index } => {
                if self.open[index] {
                    return Err(GestureError::AlreadyOpen(index));
                }
                self.open[index] = true;
            }
            EditorMessage::Set { index, .. } if !self.open[index] => {
                return Err(GestureError::NotOpen(index));
            }
            EditorMessage::End { index } => {
                if !self.open[index] {
                    return Err(GestureError::NotOpen(index));
                }
                self.open[index] = false;
            }
            EditorMessage::Commit { index, .. } if self.open[index] => {
                return Err(GestureError::AlreadyOpen(index));
            }
            _ => {}
        }
        Ok(())
    }
    pub fn is_open(&self, index: usize) -> bool {
        self.open[index]
    }
    /// Close every open gesture, returning their indices so the adapter can
    /// send the host an end for each. Call when the editor closes.
    pub fn close_all(&mut self) -> Vec<usize> {
        let open = self
            .open
            .iter()
            .enumerate()
            .filter_map(|(index, open)| open.then_some(index))
            .collect();
        self.open.fill(false);
        open
    }
}

/// The `info` message: product and parameter metadata for building the editor.
pub fn info(product: &ResolvedProduct<'_>) -> String {
    let description = product.product();
    let parameters: Vec<Value> = description
        .parameters()
        .iter()
        .zip(product.parameter_ids())
        .map(|(parameter, id)| {
            let range = parameter.range();
            json!({
                "key": parameter.key(),
                "id": id.0,
                "name": parameter.name(),
                "unit": parameter.unit(),
                "minimum": range.minimum(),
                "maximum": range.maximum(),
                "scale": match range.scale() {
                    ParameterScale::Linear => "linear",
                    ParameterScale::Logarithmic => "logarithmic",
                },
                "steps": range.intervals(),
                "default": parameter.default(),
                "bypass": parameter.role() == ParameterRole::Bypass,
                "automatable": parameter.is_automatable(),
                "hidden": parameter.is_hidden(),
                "readOnly": parameter.is_read_only(),
            })
        })
        .collect();
    json!({
        "type": "info",
        "protocol": PROTOCOL_VERSION,
        "product": {
            "name": description.name(),
            "vendor": description.vendor(),
            "version": description.version(),
            "category": match description.category() {
                Category::Effect => "effect",
                Category::Instrument => "instrument",
                Category::MidiEffect => "midi-effect",
            },
        },
        "parameters": parameters,
    })
    .to_string()
}

fn values_by_key(
    product: &ResolvedProduct<'_>,
    values: impl Iterator<Item = (usize, f32)>,
) -> Map<String, Value> {
    let parameters = product.product().parameters();
    values
        .filter(|(_, value)| value.is_finite())
        .map(|(index, value)| (parameters[index].key().to_owned(), json!(value)))
        .collect()
}

/// The `snapshot` message with every parameter value in declaration order.
/// Nonfinite values are omitted; they cannot occur in validated state.
pub fn snapshot(
    product: &ResolvedProduct<'_>,
    sequence: u64,
    values: &[f32],
    telemetry: Option<&Value>,
    product_data: Option<&Value>,
) -> String {
    let mut message = json!({
        "type": "snapshot",
        "sequence": sequence,
        "parameters": values_by_key(product, values.iter().copied().enumerate()),
    });
    if let Some(telemetry) = telemetry {
        message["telemetry"] = telemetry.clone();
    }
    if let Some(data) = product_data {
        message["product"] = data.clone();
    }
    message.to_string()
}

/// The `changes` message with changed values as (declaration index, value).
pub fn changes(product: &ResolvedProduct<'_>, sequence: u64, changed: &[(usize, f32)]) -> String {
    json!({
        "type": "changes",
        "sequence": sequence,
        "parameters": values_by_key(product, changed.iter().copied()),
    })
    .to_string()
}

/// A product-defined message to the editor.
pub fn product_message(data: &Value) -> String {
    json!({ "type": "product", "data": data }).to_string()
}

/// The editor client script injected into every editor page.
pub const CLIENT_SCRIPT: &str = include_str!("../editor/client.js");

/// The product's page with the editor client injected at the start of `<head>`,
/// or at the start of the document when there is no `<head>`.
pub fn page(html: &str) -> String {
    let script = format!("<script>{CLIENT_SCRIPT}</script>");
    let lower = html.to_ascii_lowercase();
    match lower
        .find("<head")
        .and_then(|start| lower[start..].find('>').map(|end| start + end + 1))
    {
        Some(position) => format!("{}{script}{}", &html[..position], &html[position..]),
        None => format!("{script}{html}"),
    }
}
