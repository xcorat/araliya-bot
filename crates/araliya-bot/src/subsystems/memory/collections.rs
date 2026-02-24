//! Collection types for the memory subsystem.
//!
//! Collections are the primary containers agents read and write.
//! Two concrete variants are fully implemented:
//!
//! * [`Doc`] — a string-keyed map of **scalar** values ([`PrimaryValue`]).
//!   Ideal for agent config, session metadata, extracted facts, and anything
//!   that maps cleanly to bool/int/float/string.
//!
//! * [`Block`] — a string-keyed map of **any** [`Value`] (scalars or binary
//!   [`Obj`]).  Use for structured working memory, intermediate computation
//!   results, or payloads that include blobs.
//!
//! The [`Collection`] enum wraps both plus stub variants for future types
//! (Set, List, Vec, Tuple, Tensor).  Stubs compile but panic if accessed —
//! they are placeholders, not silently-lossy no-ops.
//!
//! # Design rationale
//! Separating Doc (scalars only) from Block (any value) mirrors the
//! working-memory / episodic distinction in cognitive architectures:
//! Docs hold fast, compareable facts; Blocks carry richer payloads.
//! The strict type boundary makes retrieval semantics predictable and
//! keeps the most common case (scalar fact store) maximally cheap.

use std::collections::HashMap;

use super::types::{PrimaryValue, Value};

// ── Doc ───────────────────────────────────────────────────────────────────────

/// A string-keyed map of scalar [`PrimaryValue`] entries.
///
/// The primary store for agent metadata, session facts, and configuration —
/// anywhere you want assured comparability and cheap cloning.
///
/// # Examples
/// ```rust,no_run
/// use araliya_bot::subsystems::memory::collections::Doc;
/// use araliya_bot::subsystems::memory::types::PrimaryValue;
///
/// let mut doc = Doc::default();
/// doc.set("status".into(), PrimaryValue::from("active"));
/// assert_eq!(doc.get("status"), Some(&PrimaryValue::from("active")));
/// doc.delete("status");
/// assert!(doc.is_empty());
/// ```
#[derive(Debug, Clone, Default)]
pub struct Doc {
    entries: HashMap<String, PrimaryValue>,
}

impl Doc {
    /// Return a reference to the value for `key`, or `None` if absent.
    pub fn get(&self, key: &str) -> Option<&PrimaryValue> {
        self.entries.get(key)
    }

    /// Insert or overwrite the value for `key`.
    pub fn set(&mut self, key: String, value: PrimaryValue) {
        self.entries.insert(key, value);
    }

    /// Remove `key`.  Returns `true` if it was present.
    pub fn delete(&mut self, key: &str) -> bool {
        self.entries.remove(key).is_some()
    }

    /// All current keys in arbitrary order.
    pub fn keys(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    /// Number of entries.
    pub fn len(&self) -> usize { self.entries.len() }

    /// `true` when there are no entries.
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }
}

// ── Block ─────────────────────────────────────────────────────────────────────

/// A string-keyed map of [`Value`] entries (scalars or binary objects).
///
/// Higher-capacity working memory for agents that need to store blobs,
/// embeddings, or any payload that doesn't fit into a scalar.
///
/// # Examples
/// ```rust,no_run
/// use araliya_bot::subsystems::memory::collections::Block;
/// use araliya_bot::subsystems::memory::types::{Value, Obj};
///
/// let mut block = Block::default();
/// block.set("raw".into(), Value::from(Obj::new(b"data".to_vec())));
/// assert_eq!(block.get("raw").unwrap().to_string(), "<Obj 4 bytes>");
/// ```
#[derive(Debug, Clone, Default)]
pub struct Block {
    entries: HashMap<String, Value>,
}

impl Block {
    /// Return a reference to the value for `key`, or `None` if absent.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.entries.get(key)
    }

    /// Insert or overwrite the value for `key`.
    pub fn set(&mut self, key: String, value: Value) {
        self.entries.insert(key, value);
    }

    /// Remove `key`.  Returns `true` if it was present.
    pub fn delete(&mut self, key: &str) -> bool {
        self.entries.remove(key).is_some()
    }

    /// All current keys in arbitrary order.
    pub fn keys(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    /// Number of entries.
    pub fn len(&self) -> usize { self.entries.len() }

    /// `true` when there are no entries.
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }
}

// ── Collection ────────────────────────────────────────────────────────────────

/// Tagged union of all collection types.
///
/// [`Doc`] and [`Block`] are fully implemented.  All other variants are
/// **stubs** — they compile but panic on access with `unimplemented!()`.
/// This is intentional: stubs exist to reserve the namespace and make
/// future additions non-breaking.
///
/// Use the `as_*` helpers to downcast without an explicit match:
///
/// ```rust,no_run
/// use araliya_bot::subsystems::memory::collections::{Collection, Doc};
///
/// let mut c = Collection::Doc(Doc::default());
/// c.as_doc_mut().unwrap().set("k".into(), "v".into());
/// assert_eq!(c.as_doc().unwrap().get("k").unwrap().to_string(), "v");
/// ```
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Collection {
    /// Scalar fact store (see [`Doc`]).
    Doc(Doc),
    /// Rich payload store (see [`Block`]).
    Block(Block),
    /// Unordered unique scalars — not yet implemented.
    Set(()),
    /// Ordered list of values — not yet implemented.
    List(()),
    /// Dense numeric vector — not yet implemented.
    Vec(()),
    /// Fixed-arity heterogeneous sequence — not yet implemented.
    Tuple(()),
    /// Multi-dimensional numeric tensor — not yet implemented.
    Tensor(()),
}

impl Collection {
    // ── Doc accessors ─────────────────────────────────────────────────

    /// Return `Some(&Doc)` if this is a `Doc` collection, else `None`.
    pub fn as_doc(&self) -> Option<&Doc> {
        if let Collection::Doc(d) = self { Some(d) } else { None }
    }

    /// Return `Some(&mut Doc)` if this is a `Doc` collection, else `None`.
    pub fn as_doc_mut(&mut self) -> Option<&mut Doc> {
        if let Collection::Doc(d) = self { Some(d) } else { None }
    }

    // ── Block accessors ───────────────────────────────────────────────

    /// Return `Some(&Block)` if this is a `Block` collection, else `None`.
    pub fn as_block(&self) -> Option<&Block> {
        if let Collection::Block(b) = self { Some(b) } else { None }
    }

    /// Return `Some(&mut Block)` if this is a `Block` collection, else `None`.
    pub fn as_block_mut(&mut self) -> Option<&mut Block> {
        if let Collection::Block(b) = self { Some(b) } else { None }
    }

    // ── Consuming accessors ───────────────────────────────────────────

    /// Consume `self` and return the inner [`Doc`], or `None`.
    pub fn into_doc(self) -> Option<Doc> {
        if let Collection::Doc(d) = self { Some(d) } else { None }
    }

    /// Consume `self` and return the inner [`Block`], or `None`.
    pub fn into_block(self) -> Option<Block> {
        if let Collection::Block(b) = self { Some(b) } else { None }
    }

    // ── Stub accessors (panic, not silently wrong) ─────────────────────

    /// Panics — `Set` is not yet implemented.
    pub fn as_set(&self) -> ! {
        unimplemented!("Collection::Set is a stub; not implemented in this phase")
    }

    /// Panics — `List` is not yet implemented.
    pub fn as_list(&self) -> ! {
        unimplemented!("Collection::List is a stub; not implemented in this phase")
    }

    /// Panics — `Vec` is not yet implemented.
    pub fn as_vec(&self) -> ! {
        unimplemented!("Collection::Vec is a stub; not implemented in this phase")
    }

    /// Panics — `Tuple` is not yet implemented.
    pub fn as_tuple(&self) -> ! {
        unimplemented!("Collection::Tuple is a stub; not implemented in this phase")
    }

    /// Panics — `Tensor` is not yet implemented.
    pub fn as_tensor(&self) -> ! {
        unimplemented!("Collection::Tensor is a stub; not implemented in this phase")
    }

    /// Named variant string for diagnostics.
    pub fn variant_name(&self) -> &'static str {
        match self {
            Collection::Doc(_) => "Doc",
            Collection::Block(_) => "Block",
            Collection::Set(_) => "Set",
            Collection::List(_) => "List",
            Collection::Vec(_) => "Vec",
            Collection::Tuple(_) => "Tuple",
            Collection::Tensor(_) => "Tensor",
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subsystems::memory::types::{Obj, PrimaryValue, Value};

    // ── Doc tests ─────────────────────────────────────────────────────

    #[test]
    fn doc_set_get() {
        let mut doc = Doc::default();
        assert!(doc.is_empty());
        doc.set("k".into(), PrimaryValue::Int(42));
        assert_eq!(doc.get("k"), Some(&PrimaryValue::Int(42)));
        assert_eq!(doc.len(), 1);
    }

    #[test]
    fn doc_overwrite() {
        let mut doc = Doc::default();
        doc.set("k".into(), PrimaryValue::from("old"));
        doc.set("k".into(), PrimaryValue::from("new"));
        assert_eq!(doc.get("k"), Some(&PrimaryValue::from("new")));
        assert_eq!(doc.len(), 1);
    }

    #[test]
    fn doc_delete() {
        let mut doc = Doc::default();
        doc.set("x".into(), PrimaryValue::Bool(true));
        assert!(doc.delete("x"));
        assert!(!doc.delete("x")); // second delete returns false
        assert!(doc.is_empty());
    }

    #[test]
    fn doc_keys() {
        let mut doc = Doc::default();
        doc.set("a".into(), PrimaryValue::Int(1));
        doc.set("b".into(), PrimaryValue::Int(2));
        let mut keys = doc.keys();
        keys.sort();
        assert_eq!(keys, vec!["a", "b"]);
    }

    // ── Block tests ───────────────────────────────────────────────────

    #[test]
    fn block_set_get_scalar() {
        let mut block = Block::default();
        block.set("score".into(), Value::from(99i64));
        assert_eq!(block.get("score"), Some(&Value::Primary(PrimaryValue::Int(99))));
    }

    #[test]
    fn block_set_get_obj() {
        let mut block = Block::default();
        let obj = Obj::new(b"payload".to_vec());
        block.set("data".into(), Value::from(obj.clone()));
        assert_eq!(block.get("data"), Some(&Value::Obj(obj)));
    }

    #[test]
    fn block_delete_and_len() {
        let mut block = Block::default();
        block.set("x".into(), Value::from(1i64));
        block.set("y".into(), Value::from(2i64));
        assert_eq!(block.len(), 2);
        assert!(block.delete("x"));
        assert_eq!(block.len(), 1);
        assert!(!block.delete("x"));
    }

    // ── Collection enum tests ─────────────────────────────────────────

    #[test]
    fn collection_as_doc() {
        let mut c = Collection::Doc(Doc::default());
        c.as_doc_mut().unwrap().set("a".into(), PrimaryValue::Bool(true));
        assert_eq!(c.as_doc().unwrap().get("a"), Some(&PrimaryValue::Bool(true)));
        assert_eq!(c.variant_name(), "Doc");
    }

    #[test]
    fn collection_as_block() {
        let mut c = Collection::Block(Block::default());
        c.as_block_mut().unwrap().set("val".into(), Value::from("hello"));
        assert_eq!(c.as_block().unwrap().get("val"), Some(&Value::from("hello")));
        assert_eq!(c.variant_name(), "Block");
    }

    #[test]
    fn collection_wrong_downcast_returns_none() {
        let c = Collection::Doc(Doc::default());
        assert!(c.as_block().is_none());

        let c2 = Collection::Block(Block::default());
        assert!(c2.as_doc().is_none());
    }
}
