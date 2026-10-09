//! Bounded UTF-8 XML trees for attribute-based state and preset records.
//! Names use ASCII XML identifiers; attribute values may contain Unicode.

use quick_xml::{Reader, events::Event};
use std::{collections::BTreeMap, fmt};

pub const MAXIMUM_BYTES: usize = 65536;
pub const MAXIMUM_DEPTH: usize = 16;
pub const MAXIMUM_ELEMENTS: usize = 1024;

fn valid_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b) if b.is_ascii_alphabetic() || b == b'_' || b == b':')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
}

/// Attribute-based XML element. Text content and external entity declarations
/// are intentionally unsupported. Parsing and encoding belong off the DSP thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Element {
    pub name: String,
    pub attributes: BTreeMap<String, String>,
    pub children: Vec<Element>,
}

#[derive(Debug)]
pub struct XmlError(pub &'static str);
impl fmt::Display for XmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for XmlError {}

impl Element {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            attributes: BTreeMap::new(),
            children: Vec::new(),
        }
    }
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes.get(name).map(String::as_str)
    }

    pub fn parse(text: &str) -> Result<Self, XmlError> {
        if text.len() > MAXIMUM_BYTES {
            return Err(XmlError("XML exceeds byte limit"));
        }
        let mut reader = Reader::from_str(text);
        let mut stack: Vec<Self> = Vec::new();
        let mut root = None;
        let mut count = 0;
        loop {
            let event = reader.read_event().map_err(|_| XmlError("malformed XML"))?;
            match event {
                Event::Start(ref tag) | Event::Empty(ref tag) => {
                    count += 1;
                    if count > MAXIMUM_ELEMENTS || stack.len() >= MAXIMUM_DEPTH {
                        return Err(XmlError("XML exceeds tree limits"));
                    }
                    let name = std::str::from_utf8(tag.name().as_ref())
                        .map_err(|_| XmlError("invalid XML name"))?
                        .to_owned();
                    if !valid_name(&name) {
                        return Err(XmlError("invalid or non-ASCII XML name"));
                    }
                    let mut node = Self::new(name);
                    for attribute in tag.attributes() {
                        let attribute = attribute
                            .map_err(|_| XmlError("invalid or duplicate XML attribute"))?;
                        let key = std::str::from_utf8(attribute.key.as_ref())
                            .map_err(|_| XmlError("invalid attribute name"))?
                            .to_owned();
                        if !valid_name(&key) {
                            return Err(XmlError("invalid or non-ASCII attribute name"));
                        }
                        let value = attribute
                            .decode_and_unescape_value(reader.decoder())
                            .map_err(|_| XmlError("invalid attribute value"))?
                            .into_owned();
                        node.attributes.insert(key, value);
                    }
                    if matches!(event, Event::Start(_)) {
                        stack.push(node);
                    } else {
                        append(&mut stack, &mut root, node)?;
                    }
                }
                Event::End(_) => {
                    let node = stack.pop().ok_or(XmlError("unbalanced XML"))?;
                    append(&mut stack, &mut root, node)?;
                }
                Event::Text(text) => {
                    if !text.as_ref().iter().all(u8::is_ascii_whitespace) {
                        return Err(XmlError("text XML is unsupported"));
                    }
                }
                Event::Decl(_) | Event::Comment(_) => {}
                Event::Eof => break,
                _ => {
                    return Err(XmlError(
                        "XML entities, processing instructions, and CDATA are unsupported",
                    ));
                }
            }
        }
        if !stack.is_empty() {
            return Err(XmlError("unclosed XML element"));
        }
        root.ok_or(XmlError("empty XML document"))
    }

    /// Serialize an attribute tree and enforce the same limits by parsing the
    /// result. Arbitrary element/attribute names must be valid XML identifiers.
    pub fn encode(&self) -> Result<String, XmlError> {
        fn write(node: &Element, out: &mut String, depth: usize) -> Result<(), XmlError> {
            if depth >= MAXIMUM_DEPTH {
                return Err(XmlError("XML exceeds depth limit"));
            }
            if !valid_name(&node.name) || node.name.len() > MAXIMUM_BYTES {
                return Err(XmlError("invalid XML name"));
            }
            out.push('<');
            out.push_str(&node.name);
            for (key, value) in &node.attributes {
                if !valid_name(key)
                    || key
                        .len()
                        .saturating_add(value.len())
                        .saturating_add(out.len())
                        > MAXIMUM_BYTES
                {
                    return Err(XmlError("invalid attribute or XML exceeds byte limit"));
                }
                out.push(' ');
                out.push_str(key);
                out.push_str("=\"");
                out.push_str(&quick_xml::escape::escape(value));
                out.push('"');
                if out.len() > MAXIMUM_BYTES {
                    return Err(XmlError("XML exceeds byte limit"));
                }
            }
            if node.children.is_empty() {
                out.push_str("/>");
            } else {
                out.push('>');
                for child in &node.children {
                    write(child, out, depth + 1)?;
                }
                out.push_str("</");
                out.push_str(&node.name);
                out.push('>');
            }
            if out.len() > MAXIMUM_BYTES {
                return Err(XmlError("XML exceeds byte limit"));
            }
            Ok(())
        }
        let mut out = String::new();
        write(self, &mut out, 0)?;
        let decoded = Self::parse(&out)?;
        if decoded != *self {
            return Err(XmlError("invalid XML tree names"));
        }
        Ok(out)
    }
}

fn append(
    stack: &mut [Element],
    root: &mut Option<Element>,
    node: Element,
) -> Result<(), XmlError> {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    } else if root.is_some() {
        return Err(XmlError("multiple XML roots"));
    } else {
        *root = Some(node);
    }
    Ok(())
}
