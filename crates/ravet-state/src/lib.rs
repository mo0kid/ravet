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
use std::collections::{BTreeMap, BTreeSet, VecDeque};

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
    /// Version-one complete preset. No DSP history, undo history or cursors.
    /// Product identifier is caller-owned; the format is deterministic.
    pub fn encode(&self, product: [u8; 8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RVETTREE");
        bytes.extend_from_slice(&product);
        bytes.extend_from_slice(&1u32.to_le_bytes());
        encode_node(&self.root, &mut bytes);
        bytes
    }
    /// Decode and apply a complete preset as one undoable transaction.
    /// Bad records and net no-op recalls preserve all live state/history.
    pub fn restore(&mut self, bytes: &[u8], product: [u8; 8]) -> Result<bool, StateError> {
        let decoded = Self::decode(bytes, product, self.limits)?;
        self.commit(&[Mutation::ReplaceRoot(decoded.root)])
    }
    /// Strictly decode a bounded preset without touching a live store.
    /// Reject wrong identity/version, trailing bytes, duplicate IDs/properties,
    /// malformed UTF-8, nonfinite values, excessive depth/nodes and invalid names.
    pub fn decode(bytes: &[u8], product: [u8; 8], limits: Limits) -> Result<Self, StateError> {
        validate_limits(limits)?;
        if bytes.len() > limits.bytes {
            return Err(StateError::Capacity);
        }
        if bytes.len() < 20 || &bytes[..8] != b"RVETTREE" || bytes[16..20] != 1u32.to_le_bytes() {
            return Err(StateError::InvalidRecord);
        }
        if bytes[8..16] != product {
            return Err(StateError::WrongProduct);
        }
        let mut reader = Reader {
            bytes: &bytes[20..],
            limits,
            nodes: 0,
        };
        let root = reader.node(1)?;
        if !reader.bytes.is_empty() {
            return Err(StateError::InvalidRecord);
        }
        Self::new(root, limits)
    }
}
fn validate_limits(l: Limits) -> Result<(), StateError> {
    if l.nodes == 0
        || l.nodes > 4096
        || l.depth == 0
        || l.depth > 64
        || l.bytes < 20
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
    let mut bytes = 20;
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
