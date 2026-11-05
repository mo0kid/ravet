//! Owned observable trees and transactional undo on control threads.
//!
//! This crate allocates and clones bounded snapshots; never use it in an audio
//! callback. Publish validated processor-specific snapshots through the portable
//! control/audio bridge. Stable node IDs are product-owned and never renumbered.
//!
//! ```
//! use ravet_state::{Limits, Mutation, Node, NodeId, Store, Value};
//! let mut state = Store::new(Node::new(NodeId(1), "session"), Limits::default())?;
//! state.commit(&[Mutation::Set {
//!     node: NodeId(1), key: "gain".into(), value: Value::Real(0.5),
//! }])?;
//! let preset = state.encode(*b"PRODUCT1");
//! state.undo()?;
//! state.restore(&preset, *b"PRODUCT1")?;
//! # Ok::<(), ravet_state::StateError>(())
//! ```
//!
//! # Record versions
//!
//! Every `RVETTREE` record carries a record format version, and version-two
//! records also carry the product's own schema version. Decoding rejects record
//! formats newer than [`TREE_FORMAT_VERSION`] with
//! [`StateError::UnsupportedVersion`], and product schemas newer than the
//! caller's current schema with [`StateError::UnsupportedSchema`]; neither is
//! ever reinterpreted. Version-one records (RAVET 0.0.1) still decode, as
//! product schema 1. Products migrate older schemas themselves:
//!
//! ```
//! use core::num::NonZeroU32;
//! use ravet_state::{Limits, Node, NodeId, Store, Value};
//! let v1 = NonZeroU32::new(1).unwrap();
//! let v2 = NonZeroU32::new(2).unwrap();
//! let old = Store::new(Node::new(NodeId(1), "session")
//!     .with_property("gain", Value::Real(0.5)), Limits::default())?;
//! let bytes = old.encode_schema(*b"PRODUCT1", v1);
//! let mut live = Store::new(Node::new(NodeId(1), "session"), Limits::default())?;
//! live.restore_migrating(&bytes, *b"PRODUCT1", v2, |schema, mut root| {
//!     if schema == 1 {
//!         // Schema 2 renamed "gain" to "level".
//!         root = Node::new(root.id(), root.name()).with_property(
//!             "level", root.properties().get("gain").cloned().unwrap_or(Value::Null));
//!     }
//!     Ok(root)
//! })?;
//! assert_eq!(live.root().properties().get("level"), Some(&Value::Real(0.5)));
//! # Ok::<(), ravet_state::StateError>(())
//! ```
use core::num::NonZeroU32;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Magic bytes at the start of every observable-tree record.
pub const TREE_MAGIC: [u8; 8] = *b"RVETTREE";
/// Record format version written by this release. Version one (RAVET 0.0.1)
/// had no product schema field; it is still read, as product schema 1.
pub const TREE_FORMAT_VERSION: u32 = 2;
/// Product schema version used by [`Store::encode`], [`Store::decode`] and
/// [`Store::restore`].
pub const DEFAULT_SCHEMA: NonZeroU32 = NonZeroU32::MIN;
const HEADER_V1: usize = 20;
const HEADER_V2: usize = 24;

/// The versions stored at the start of a tree record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeHeader {
    /// Record format version: 1 or 2.
    pub format: u32,
    /// Product schema version; 1 for version-one records.
    pub schema: NonZeroU32,
}

/// Read and check the record header without decoding the tree.
///
/// Rejects foreign magic and truncated headers ([`StateError::InvalidRecord`]),
/// another product ([`StateError::WrongProduct`]) and record formats other than
/// 1 and 2 ([`StateError::UnsupportedVersion`]). A zero schema is malformed.
pub fn read_header(bytes: &[u8], product: [u8; 8]) -> Result<TreeHeader, StateError> {
    if bytes.len() < HEADER_V1 || bytes[..8] != TREE_MAGIC {
        return Err(StateError::InvalidRecord);
    }
    if bytes[8..16] != product {
        return Err(StateError::WrongProduct);
    }
    let format = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    let schema = match format {
        1 => DEFAULT_SCHEMA,
        2 => {
            let raw = bytes.get(20..24).ok_or(StateError::InvalidRecord)?;
            NonZeroU32::new(u32::from_le_bytes(raw.try_into().unwrap()))
                .ok_or(StateError::InvalidRecord)?
        }
        other => return Err(StateError::UnsupportedVersion(other)),
    };
    Ok(TreeHeader { format, schema })
}

/// Product-owned stable node identity; zero has no special meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeId(pub u64);
/// Persistable property values. Floating values must be finite.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Integer(i64),
    Real(f64),
    Text(String),
    Bytes(Vec<u8>),
}
/// Owned tree node. Builders are validated when admitted to a store.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    id: NodeId,
    name: String,
    properties: BTreeMap<String, Value>,
    children: Vec<Node>,
}
impl Node {
    pub fn new(id: NodeId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            properties: BTreeMap::new(),
            children: Vec::new(),
        }
    }
    pub fn with_property(mut self, key: impl Into<String>, value: Value) -> Self {
        self.properties.insert(key.into(), value);
        self
    }
    pub fn with_child(mut self, child: Node) -> Self {
        self.children.push(child);
        self
    }
    pub fn id(&self) -> NodeId {
        self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn properties(&self) -> &BTreeMap<String, Value> {
        &self.properties
    }
    pub fn children(&self) -> &[Node] {
        &self.children
    }
    pub fn find(&self, id: NodeId) -> Option<&Node> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|n| n.find(id))
    }
    fn find_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter_mut().find_map(|n| n.find_mut(id))
    }
    fn remove(&mut self, id: NodeId) -> Option<Node> {
        if let Some(index) = self.children.iter().position(|n| n.id == id) {
            return Some(self.children.remove(index));
        }
        self.children.iter_mut().find_map(|n| n.remove(id))
    }
}
/// Complete transaction operations; order is significant.
#[derive(Clone, Debug, PartialEq)]
pub enum Mutation {
    /// Replace the complete tree, including its root ID, as one undoable edit.
    ReplaceRoot(Node),
    Set {
        node: NodeId,
        key: String,
        value: Value,
    },
    RemoveProperty {
        node: NodeId,
        key: String,
    },
    Insert {
        parent: NodeId,
        index: usize,
        child: Node,
    },
    Remove {
        node: NodeId,
    },
    /// Index is measured in the destination after removing the source.
    Move {
        node: NodeId,
        parent: NodeId,
        index: usize,
    },
}
/// Validated caps. Maximums: 4096 nodes, depth 64, 16 MiB encoded bytes,
/// 128 undo snapshots and 128 journal batches. Journal must be nonempty.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub nodes: usize,
    pub depth: usize,
    pub bytes: usize,
    pub history: usize,
    pub journal: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            nodes: 4096,
            depth: 64,
            bytes: 16 * 1024 * 1024,
            history: 64,
            journal: 64,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateError {
    InvalidLimits,
    InvalidNode,
    DuplicateId(NodeId),
    MissingNode(NodeId),
    InvalidOperation,
    Capacity,
    InvalidRecord,
    WrongProduct,
    RevisionExhausted,
    /// The record format version is newer than this release, or unknown.
    UnsupportedVersion(u32),
    /// The product schema version is newer than the caller's current schema.
    UnsupportedSchema(u32),
}
impl std::fmt::Display for StateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "state error: {self:?}")
    }
}
impl std::error::Error for StateError {}
/// One revision, representing a complete committed transaction or restoration.
#[derive(Clone, Debug, PartialEq)]
pub struct ChangeBatch {
    pub revision: u64,
    pub change: Change,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Edits(Vec<Mutation>),
    Undo,
    Redo,
}
/// Poll cursor associated with one store. Construct through `subscribe`.
/// Do not transfer cursors to a different store.
#[derive(Debug)]
pub struct Subscription {
    revision: u64,
}
/// Bounded notifications. A lagging cursor receives the complete current tree.
#[derive(Debug)]
pub enum Notifications<'a> {
    Changes(Vec<&'a ChangeBatch>),
    Resync { revision: u64, root: &'a Node },
}
/// Single-owner control-thread store, with atomic transactions and bounded history.
pub struct Store {
    root: Node,
    limits: Limits,
    revision: u64,
    undo: VecDeque<Node>,
    redo: VecDeque<Node>,
    journal: VecDeque<ChangeBatch>,
}
impl Store {
    pub fn new(root: Node, limits: Limits) -> Result<Self, StateError> {
        validate_limits(limits)?;
        validate(&root, limits)?;
        Ok(Self {
            root,
            limits,
            revision: 0,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            journal: VecDeque::new(),
        })
    }
    pub fn root(&self) -> &Node {
        &self.root
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    /// Subscribe after current state; obtain that state with `root` first.
    pub fn subscribe(&self) -> Subscription {
        Subscription {
            revision: self.revision,
        }
    }
    /// Poll complete batches and advance cursor. Overrun returns a resync snapshot.
    /// Returned references prevent mutation until the notifications are dropped.
    pub fn poll<'a>(&'a self, subscription: &mut Subscription) -> Notifications<'a> {
        let revision = subscription.revision;
        subscription.revision = self.revision;
        let first = self
            .journal
            .front()
            .map_or(self.revision, |b| b.revision.saturating_sub(1));
        if revision < first || revision > self.revision {
            Notifications::Resync {
                revision: self.revision,
                root: &self.root,
            }
        } else {
            Notifications::Changes(
                self.journal
                    .iter()
                    .filter(|b| b.revision > revision)
                    .collect(),
            )
        }
    }
    /// Validate and apply all edits on an owned shadow before publishing.
    /// Failure and net no-op preserve tree, revision, redo, history and journal.
    /// Transaction count is bounded to 4096 before cloning operation payloads.
    pub fn commit(&mut self, edits: &[Mutation]) -> Result<bool, StateError> {
        if edits.len() > 4096 {
            return Err(StateError::Capacity);
        }
        // Bound journal payloads before cloning any caller-owned data.
        let mut payload = 0usize;
        for edit in edits {
            let size = match edit {
                Mutation::Insert { child, .. } | Mutation::ReplaceRoot(child) => {
                    validate(child, self.limits)?
                }
                Mutation::Set { key, value, .. } => property_size(key, value)?,
                Mutation::RemoveProperty { key, .. } => {
                    if key.is_empty() || key.len() > 256 {
                        return Err(StateError::InvalidNode);
                    }
                    key.len() + 8
                }
                _ => 32,
            };
            payload = payload.checked_add(size).ok_or(StateError::Capacity)?;
            if payload > self.limits.bytes {
                return Err(StateError::Capacity);
            }
        }
        let mut next = self.root.clone();
        for edit in edits {
            apply(&mut next, edit)?;
            // Bound intermediate transactions, including consecutive inserts.
            validate(&next, self.limits)?;
        }
        if next == self.root {
            return Ok(false);
        }
        self.ensure_revision()?;
        if self.limits.history != 0 {
            if self.undo.len() == self.limits.history {
                self.undo.pop_front();
            }
            self.undo.push_back(self.root.clone());
        }
        self.root = next;
        self.redo.clear();
        self.record(Change::Edits(edits.to_vec()));
        Ok(true)
    }
    pub fn undo(&mut self) -> Result<bool, StateError> {
        if self.undo.is_empty() {
            return Ok(false);
        }
        self.ensure_revision()?;
        let previous = self.undo.pop_back().unwrap();
        self.redo
            .push_back(std::mem::replace(&mut self.root, previous));
        self.record(Change::Undo);
        Ok(true)
    }
    pub fn redo(&mut self) -> Result<bool, StateError> {
        if self.redo.is_empty() {
            return Ok(false);
        }
        self.ensure_revision()?;
        let next = self.redo.pop_back().unwrap();
        self.undo.push_back(std::mem::replace(&mut self.root, next));
        self.record(Change::Redo);
        Ok(true)
    }
    fn ensure_revision(&self) -> Result<(), StateError> {
        if self.revision == u64::MAX {
            Err(StateError::RevisionExhausted)
        } else {
            Ok(())
        }
    }
    fn record(&mut self, change: Change) {
        self.revision += 1;
        if self.journal.len() == self.limits.journal {
            self.journal.pop_front();
        }
        self.journal.push_back(ChangeBatch {
            revision: self.revision,
            change,
        });
    }
    /// Complete preset at [`DEFAULT_SCHEMA`]; see [`Store::encode_schema`].
    pub fn encode(&self, product: [u8; 8]) -> Vec<u8> {
        self.encode_schema(product, DEFAULT_SCHEMA)
    }
    /// Version-two complete preset carrying a product schema version. No DSP
    /// history, undo history or cursors. The product identifier is caller-owned;
    /// the format is deterministic.
    pub fn encode_schema(&self, product: [u8; 8], schema: NonZeroU32) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&TREE_MAGIC);
        bytes.extend_from_slice(&product);
        bytes.extend_from_slice(&TREE_FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&schema.get().to_le_bytes());
        encode_node(&self.root, &mut bytes);
        bytes
    }
    /// Decode and apply a complete preset as one undoable transaction.
    /// Bad records and net no-op recalls preserve all live state/history.
    /// Only records at [`DEFAULT_SCHEMA`] (and version-one records) are accepted.
    pub fn restore(&mut self, bytes: &[u8], product: [u8; 8]) -> Result<bool, StateError> {
        let decoded = Self::decode(bytes, product, self.limits)?;
        self.commit(&[Mutation::ReplaceRoot(decoded.root)])
    }
    /// Decode, migrate and apply a complete preset as one undoable transaction.
    ///
    /// `migrate` receives the saved schema and decoded root only when the saved
    /// schema is older than `current`; it returns the root in the current schema.
    /// The migrated root is validated against this store's limits before
    /// anything is applied. Failures preserve live state and history.
    pub fn restore_migrating(
        &mut self,
        bytes: &[u8],
        product: [u8; 8],
        current: NonZeroU32,
        migrate: impl FnOnce(u32, Node) -> Result<Node, StateError>,
    ) -> Result<bool, StateError> {
        let (decoded, schema) = Self::decode_schema(bytes, product, current, self.limits)?;
        let root = if schema < current {
            let root = migrate(schema.get(), decoded.root)?;
            validate(&root, self.limits)?;
            root
        } else {
            decoded.root
        };
        self.commit(&[Mutation::ReplaceRoot(root)])
    }
    /// Strictly decode a bounded preset at [`DEFAULT_SCHEMA`] without touching a
    /// live store. Reject wrong identity, newer formats or schemas, trailing bytes,
    /// duplicate IDs/properties, malformed UTF-8, nonfinite values, excessive
    /// depth/nodes and invalid names.
    pub fn decode(bytes: &[u8], product: [u8; 8], limits: Limits) -> Result<Self, StateError> {
        Self::decode_schema(bytes, product, DEFAULT_SCHEMA, limits).map(|(store, _)| store)
    }
    /// Strictly decode a bounded preset whose schema is at most `current`, and
    /// return the saved schema. Older schemas are returned unchanged for the
    /// caller to migrate; newer ones reject with [`StateError::UnsupportedSchema`].
    pub fn decode_schema(
        bytes: &[u8],
        product: [u8; 8],
        current: NonZeroU32,
        limits: Limits,
    ) -> Result<(Self, NonZeroU32), StateError> {
        validate_limits(limits)?;
        if bytes.len() > limits.bytes {
            return Err(StateError::Capacity);
        }
        let header = read_header(bytes, product)?;
        if header.schema > current {
            return Err(StateError::UnsupportedSchema(header.schema.get()));
        }
        let start = if header.format == 1 {
            HEADER_V1
        } else {
            HEADER_V2
        };
        let mut reader = Reader {
            bytes: &bytes[start..],
            limits,
            nodes: 0,
        };
        let root = reader.node(1)?;
        if !reader.bytes.is_empty() {
            return Err(StateError::InvalidRecord);
        }
        Ok((Self::new(root, limits)?, header.schema))
    }
}
fn validate_limits(l: Limits) -> Result<(), StateError> {
    if l.nodes == 0
        || l.nodes > 4096
        || l.depth == 0
        || l.depth > 64
        || l.bytes < HEADER_V2
        || l.bytes > 16 * 1024 * 1024
        || l.history > 128
        || l.journal == 0
        || l.journal > 128
    {
        Err(StateError::InvalidLimits)
    } else {
        Ok(())
    }
}
fn validate(root: &Node, limits: Limits) -> Result<usize, StateError> {
    fn visit(
        n: &Node,
        depth: usize,
        ids: &mut BTreeSet<NodeId>,
        bytes: &mut usize,
        l: Limits,
    ) -> Result<(), StateError> {
        if depth > l.depth || ids.len() >= l.nodes {
            return Err(StateError::Capacity);
        }
        if !ids.insert(n.id) {
            return Err(StateError::DuplicateId(n.id));
        }
        if n.name.is_empty() || n.name.len() > 256 || n.properties.len() > 256 {
            return Err(StateError::InvalidNode);
        }
        *bytes += 8 + 4 + n.name.len() + 4 + 4;
        for (key, value) in &n.properties {
            *bytes = bytes
                .checked_add(property_size(key, value)?)
                .ok_or(StateError::Capacity)?;
        }
        if *bytes > l.bytes {
            return Err(StateError::Capacity);
        }
        for child in &n.children {
            visit(child, depth + 1, ids, bytes, l)?;
        }
        Ok(())
    }
    let mut bytes = HEADER_V2;
    visit(root, 1, &mut BTreeSet::new(), &mut bytes, limits)?;
    Ok(bytes)
}
fn property_size(key: &str, value: &Value) -> Result<usize, StateError> {
    if key.is_empty() || key.len() > 256 {
        return Err(StateError::InvalidNode);
    }
    let size = match value {
        Value::Null => 0,
        Value::Bool(_) => 1,
        Value::Integer(_) => 8,
        Value::Real(v) if v.is_finite() => 8,
        Value::Real(_) => return Err(StateError::InvalidNode),
        Value::Text(s) => s.len().checked_add(4).ok_or(StateError::Capacity)?,
        Value::Bytes(v) => v.len().checked_add(4).ok_or(StateError::Capacity)?,
    };
    size.checked_add(4 + key.len() + 1)
        .ok_or(StateError::Capacity)
}
fn apply(root: &mut Node, edit: &Mutation) -> Result<(), StateError> {
    match edit {
        Mutation::ReplaceRoot(next) => {
            *root = next.clone();
        }
        Mutation::Set { node, key, value } => {
            root.find_mut(*node)
                .ok_or(StateError::MissingNode(*node))?
                .properties
                .insert(key.clone(), value.clone());
        }
        Mutation::RemoveProperty { node, key } => {
            root.find_mut(*node)
                .ok_or(StateError::MissingNode(*node))?
                .properties
                .remove(key);
        }
        Mutation::Insert {
            parent,
            index,
            child,
        } => insert(root, *parent, *index, child.clone())?,
        Mutation::Remove { node } => {
            if root.id == *node {
                return Err(StateError::InvalidOperation);
            }
            root.remove(*node).ok_or(StateError::MissingNode(*node))?;
        }
        Mutation::Move {
            node,
            parent,
            index,
        } => {
            if root.id == *node {
                return Err(StateError::InvalidOperation);
            }
            let child = root.remove(*node).ok_or(StateError::MissingNode(*node))?;
            if child.find(*parent).is_some() {
                return Err(StateError::InvalidOperation);
            }
            insert(root, *parent, *index, child)?;
        }
    }
    Ok(())
}
fn insert(root: &mut Node, parent: NodeId, index: usize, child: Node) -> Result<(), StateError> {
    let parent = root
        .find_mut(parent)
        .ok_or(StateError::MissingNode(parent))?;
    if index > parent.children.len() {
        return Err(StateError::InvalidOperation);
    }
    parent.children.insert(index, child);
    Ok(())
}
fn blob(bytes: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(bytes);
}
fn encode_node(node: &Node, out: &mut Vec<u8>) {
    out.extend_from_slice(&node.id.0.to_le_bytes());
    blob(node.name.as_bytes(), out);
    out.extend_from_slice(&(node.properties.len() as u32).to_le_bytes());
    for (key, value) in &node.properties {
        blob(key.as_bytes(), out);
        match value {
            Value::Null => out.push(0),
            Value::Bool(v) => {
                out.push(1);
                out.push(u8::from(*v));
            }
            Value::Integer(v) => {
                out.push(2);
                out.extend_from_slice(&v.to_le_bytes());
            }
            Value::Real(v) => {
                out.push(3);
                out.extend_from_slice(&v.to_le_bytes());
            }
            Value::Text(v) => {
                out.push(4);
                blob(v.as_bytes(), out);
            }
            Value::Bytes(v) => {
                out.push(5);
                blob(v, out);
            }
        }
    }
    out.extend_from_slice(&(node.children.len() as u32).to_le_bytes());
    for child in &node.children {
        encode_node(child, out);
    }
}
struct Reader<'a> {
    bytes: &'a [u8],
    limits: Limits,
    nodes: usize,
}
impl Reader<'_> {
    fn take(&mut self, len: usize) -> Result<&[u8], StateError> {
        if len > self.bytes.len() {
            return Err(StateError::InvalidRecord);
        }
        let (part, rest) = self.bytes.split_at(len);
        self.bytes = rest;
        Ok(part)
    }
    fn number(&mut self) -> Result<u32, StateError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn data(&mut self) -> Result<Vec<u8>, StateError> {
        let len = self.number()? as usize;
        Ok(self.take(len)?.to_vec())
    }
    fn text(&mut self) -> Result<String, StateError> {
        String::from_utf8(self.data()?).map_err(|_| StateError::InvalidRecord)
    }
    fn node(&mut self, depth: usize) -> Result<Node, StateError> {
        if depth > self.limits.depth || self.nodes >= self.limits.nodes {
            return Err(StateError::Capacity);
        }
        self.nodes += 1;
        let id = NodeId(u64::from_le_bytes(self.take(8)?.try_into().unwrap()));
        let mut node = Node::new(id, self.text()?);
        let count = self.number()? as usize;
        if count > 256 {
            return Err(StateError::Capacity);
        }
        for _ in 0..count {
            let key = self.text()?;
            let value = match self.take(1)?[0] {
                0 => Value::Null,
                1 => match self.take(1)?[0] {
                    0 => Value::Bool(false),
                    1 => Value::Bool(true),
                    _ => return Err(StateError::InvalidRecord),
                },
                2 => Value::Integer(i64::from_le_bytes(self.take(8)?.try_into().unwrap())),
                3 => Value::Real(f64::from_le_bytes(self.take(8)?.try_into().unwrap())),
                4 => Value::Text(self.text()?),
                5 => Value::Bytes(self.data()?),
                _ => return Err(StateError::InvalidRecord),
            };
            if node.properties.insert(key, value).is_some() {
                return Err(StateError::InvalidRecord);
            }
        }
        let count = self.number()? as usize;
        if count > self.limits.nodes - self.nodes {
            return Err(StateError::Capacity);
        }
        for _ in 0..count {
            node.children.push(self.node(depth + 1)?);
        }
        Ok(node)
    }
}
