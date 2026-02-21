//! Core value types for the memory subsystem.
//!
//! Three-level type hierarchy:
//!
//! * [`PrimaryValue`] — scalar primitives (bool, int, float, string).
//!   These are the currency of [`Doc`](super::collections::Doc) entries
//!   and are cheap to clone, hash, and compare.
//!
//! * [`Obj`] — opaque binary with a string-keyed metadata sidecar.
//!   Use this for blobs, embeddings, images, or any data that doesn't
//!   reduce cleanly to a scalar.
//!
//! * [`Value`] — the union type accepted by
//!   [`Block`](super::collections::Block) entries.  Either a scalar or an
//!   object.
//!
//! # Design notes
//! Keeping primitives in their own enum lets collections that only need
//! scalars (Doc) enforce that constraint at the type level while allowing
//! richer collections (Block) to opt into Obj payloads without every call
//! site knowing about binary blobs.
//!
//! `Float` equality and hashing are based on bit patterns via `f64::to_bits()`
//! (no extra crates needed).  NaN values are treated as equal to themselves —
//! acceptable for in-memory metadata stores where NaN should never appear.

use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};

// ── PrimaryValue ─────────────────────────────────────────────────────────────

/// A scalar value suitable for indexing, hashing, and equality comparison.
///
/// Used as the element type for [`Doc`](super::collections::Doc) entries and
/// as the "simple" branch of [`Value`].
///
/// # Examples
/// ```rust
/// use crate::subsystems::memory::types::PrimaryValue;
///
/// let v = PrimaryValue::Str("hello".into());
/// assert_eq!(v.to_string(), "hello");
/// ```
#[derive(Debug, Clone)]
pub enum PrimaryValue {
    Bool(bool),
    Int(i64),
    /// Floating-point scalar. Equality and hashing use bit representation.
    Float(f64),
    Str(String),
}

impl PartialEq for PrimaryValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (PrimaryValue::Bool(a), PrimaryValue::Bool(b)) => a == b,
            (PrimaryValue::Int(a), PrimaryValue::Int(b)) => a == b,
            (PrimaryValue::Float(a), PrimaryValue::Float(b)) => a.to_bits() == b.to_bits(),
            (PrimaryValue::Str(a), PrimaryValue::Str(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for PrimaryValue {}

impl Hash for PrimaryValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Discriminant prefix ensures cross-variant non-collision.
        match self {
            PrimaryValue::Bool(b) => { 0u8.hash(state); b.hash(state); }
            PrimaryValue::Int(i) => { 1u8.hash(state); i.hash(state); }
            PrimaryValue::Float(f) => { 2u8.hash(state); f.to_bits().hash(state); }
            PrimaryValue::Str(s) => { 3u8.hash(state); s.hash(state); }
        }
    }
}

impl fmt::Display for PrimaryValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrimaryValue::Bool(b) => write!(f, "{b}"),
            PrimaryValue::Int(i) => write!(f, "{i}"),
            PrimaryValue::Float(v) => write!(f, "{v}"),
            PrimaryValue::Str(s) => write!(f, "{s}"),
        }
    }
}

impl From<bool> for PrimaryValue {
    fn from(v: bool) -> Self { PrimaryValue::Bool(v) }
}
impl From<i64> for PrimaryValue {
    fn from(v: i64) -> Self { PrimaryValue::Int(v) }
}
impl From<f64> for PrimaryValue {
    fn from(v: f64) -> Self { PrimaryValue::Float(v) }
}
impl From<String> for PrimaryValue {
    fn from(v: String) -> Self { PrimaryValue::Str(v) }
}
impl From<&str> for PrimaryValue {
    fn from(v: &str) -> Self { PrimaryValue::Str(v.to_string()) }
}

// ── Obj ──────────────────────────────────────────────────────────────────────

/// An opaque binary payload with a string-keyed metadata sidecar.
///
/// Intended for blobs, serialised embeddings, images, or any data that
/// cannot be represented as a [`PrimaryValue`].  The `metadata` map carries
/// annotations such as MIME type, encoding, source URL, or content hash.
///
/// # Examples
/// ```rust
/// use crate::subsystems::memory::types::Obj;
///
/// let mut obj = Obj::new(b"hello world".to_vec());
/// obj.metadata.insert("mime".into(), "text/plain".into());
/// assert_eq!(obj.metadata["mime"], "text/plain");
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Obj {
    /// Raw binary payload.
    pub data: Vec<u8>,
    /// Free-form string metadata (MIME type, encoding, provenance, …).
    pub metadata: HashMap<String, String>,
}

impl Obj {
    /// Create a new Obj with the given binary payload and empty metadata.
    pub fn new(data: Vec<u8>) -> Self {
        Self { data, metadata: HashMap::new() }
    }

    /// Byte length of the payload.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// `true` when the payload is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

// ── Value ─────────────────────────────────────────────────────────────────────

/// A value stored in a [`Block`](super::collections::Block) entry.
///
/// Either a cheap, comparable [`PrimaryValue`] or a heavier [`Obj`] payload.
/// Callers that only care about scalars can match the `Primary` arm;
/// callers that handle blobs match `Obj`.
///
/// # Examples
/// ```rust
/// use crate::subsystems::memory::types::{Value, PrimaryValue};
///
/// let v: Value = PrimaryValue::Int(42).into();
/// assert_eq!(v.to_string(), "42");
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Primary(PrimaryValue),
    Obj(Obj),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Primary(p) => write!(f, "{p}"),
            Value::Obj(o) => write!(f, "<Obj {} bytes>", o.len()),
        }
    }
}

impl From<PrimaryValue> for Value {
    fn from(p: PrimaryValue) -> Self { Value::Primary(p) }
}
impl From<Obj> for Value {
    fn from(o: Obj) -> Self { Value::Obj(o) }
}
impl From<bool> for Value {
    fn from(v: bool) -> Self { Value::Primary(PrimaryValue::Bool(v)) }
}
impl From<i64> for Value {
    fn from(v: i64) -> Self { Value::Primary(PrimaryValue::Int(v)) }
}
impl From<f64> for Value {
    fn from(v: f64) -> Self { Value::Primary(PrimaryValue::Float(v)) }
}
impl From<String> for Value {
    fn from(v: String) -> Self { Value::Primary(PrimaryValue::Str(v)) }
}
impl From<&str> for Value {
    fn from(v: &str) -> Self { Value::Primary(PrimaryValue::Str(v.to_string())) }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn primary_value_equality() {
        assert_eq!(PrimaryValue::Int(1), PrimaryValue::Int(1));
        assert_ne!(PrimaryValue::Int(1), PrimaryValue::Int(2));
        assert_eq!(PrimaryValue::Str("a".into()), PrimaryValue::from("a"));
        assert_eq!(PrimaryValue::Bool(true), PrimaryValue::Bool(true));
        assert_ne!(PrimaryValue::Bool(true), PrimaryValue::Bool(false));
    }

    #[test]
    fn primary_value_hash() {
        let mut set = HashSet::new();
        set.insert(PrimaryValue::Int(1));
        set.insert(PrimaryValue::Int(1));
        set.insert(PrimaryValue::Str("x".into()));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn primary_value_display() {
        assert_eq!(PrimaryValue::Bool(false).to_string(), "false");
        assert_eq!(PrimaryValue::Int(-7).to_string(), "-7");
        assert_eq!(PrimaryValue::Str("hi".into()).to_string(), "hi");
    }

    #[test]
    fn float_display_and_eq() {
        let a: PrimaryValue = 3.14f64.into();
        let b: PrimaryValue = 3.14f64.into();
        assert_eq!(a, b);
        assert_eq!(a.to_string(), "3.14");
    }

    #[test]
    fn obj_len_and_is_empty() {
        let o = Obj::new(vec![]);
        assert!(o.is_empty());
        let o2 = Obj::new(b"abc".to_vec());
        assert_eq!(o2.len(), 3);
    }

    #[test]
    fn value_display() {
        let v: Value = PrimaryValue::Int(99).into();
        assert_eq!(v.to_string(), "99");
        let o = Value::Obj(Obj::new(b"hello".to_vec()));
        assert_eq!(o.to_string(), "<Obj 5 bytes>");
    }

    #[test]
    fn value_from_conversions() {
        assert!(matches!(Value::from(true), Value::Primary(PrimaryValue::Bool(true))));
        assert!(matches!(Value::from(42i64), Value::Primary(PrimaryValue::Int(42))));
        assert!(matches!(Value::from("hi"), Value::Primary(PrimaryValue::Str(_))));
    }
}
