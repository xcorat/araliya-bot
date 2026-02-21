//! Core value types for the memory subsystem.
//!
//! Four types form the value vocabulary:
//!
//! * [`PrimaryValue`] — scalar primitives (bool, int, float, string).
//!   Currency of [`Doc`](super::collections::Doc) entries; cheap to clone,
//!   hash, and compare.
//!
//! * [`Obj`] — opaque binary payload with a string-keyed metadata sidecar.
//!   Use for blobs, embeddings, images, or any data that doesn't reduce to a
//!   scalar.
//!
//! * [`TextFile`] — human-readable text with a string-keyed metadata sidecar.
//!   The UTF-8 counterpart to [`Obj`].  Use for Markdown, plain text, log
//!   snippets, transcripts, or any string-valued document payload.  Metadata
//!   typically carries `"mime"` (`"text/markdown"`, `"text/plain"`, …) and
//!   optional `"ext"` / `"ts"` hints.
//!
//! * [`Value`] — the union type accepted by
//!   [`Block`](super::collections::Block) entries.
//!   Either a scalar, a binary object, or a text file.
//!
//! # Design notes
//! Keeping primitives in their own enum lets collections that only need
//! scalars (`Doc`) enforce that constraint at the type level while allowing
//! richer collections (`Block`) to opt into `Obj` / `TextFile` payloads.
//!
//! `Float` equality and hashing use `f64::to_bits()`.  NaN == NaN here —
//! acceptable for in-memory metadata stores where NaN should never appear.

use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};

// ── PrimaryValue ─────────────────────────────────────────────────────────────

/// A scalar value suitable for indexing, hashing, and equality comparison.
///
/// Used as the element type for [`Doc`](super::collections::Doc) entries and
/// as the `Primary` branch of [`Value`].
#[derive(Debug, Clone)]
pub enum PrimaryValue {
    Bool(bool),
    Int(i64),
    /// Floating-point scalar.  Equality and hashing use bit representation.
    Float(f64),
    Str(String),
}

impl PartialEq for PrimaryValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Bool(a),  Self::Bool(b))  => a == b,
            (Self::Int(a),   Self::Int(b))   => a == b,
            (Self::Float(a), Self::Float(b)) => a.to_bits() == b.to_bits(),
            (Self::Str(a),   Self::Str(b))   => a == b,
            _ => false,
        }
    }
}

impl Eq for PrimaryValue {}

impl Hash for PrimaryValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::Bool(b)  => { 0u8.hash(state); b.hash(state); }
            Self::Int(i)   => { 1u8.hash(state); i.hash(state); }
            Self::Float(f) => { 2u8.hash(state); f.to_bits().hash(state); }
            Self::Str(s)   => { 3u8.hash(state); s.hash(state); }
        }
    }
}

impl fmt::Display for PrimaryValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(b) => write!(f, "{b}"),
            Self::Int(i)  => write!(f, "{i}"),
            Self::Float(v) => write!(f, "{v}"),
            Self::Str(s)  => write!(f, "{s}"),
        }
    }
}

impl From<bool>   for PrimaryValue { fn from(v: bool)   -> Self { Self::Bool(v) } }
impl From<i64>    for PrimaryValue { fn from(v: i64)    -> Self { Self::Int(v) } }
impl From<f64>    for PrimaryValue { fn from(v: f64)    -> Self { Self::Float(v) } }
impl From<String> for PrimaryValue { fn from(v: String) -> Self { Self::Str(v) } }
impl From<&str>   for PrimaryValue { fn from(v: &str)   -> Self { Self::Str(v.to_string()) } }

// ── Obj ──────────────────────────────────────────────────────────────────────

/// An opaque binary payload with a string-keyed metadata sidecar.
///
/// Intended for blobs, serialised embeddings, images, or any data that cannot
/// be represented as a [`PrimaryValue`] or a UTF-8 string.
#[derive(Debug, Clone, PartialEq)]
pub struct Obj {
    /// Raw binary payload.
    pub data: Vec<u8>,
    /// Free-form string metadata (MIME type, encoding, provenance, …).
    pub metadata: HashMap<String, String>,
}

impl Obj {
    /// Create a new `Obj` with the given binary payload and empty metadata.
    pub fn new(data: Vec<u8>) -> Self {
        Self { data, metadata: HashMap::new() }
    }

    /// Byte length of the payload.
    pub fn len(&self) -> usize { self.data.len() }

    /// `true` when the payload is empty.
    pub fn is_empty(&self) -> bool { self.data.is_empty() }
}

// ── TextFile ──────────────────────────────────────────────────────────────────

/// A human-readable text payload with a string-keyed metadata sidecar.
///
/// The UTF-8 counterpart to [`Obj`].  Use for Markdown, plain text, log
/// snippets, transcripts, or any string content that is meaningfully human-
/// readable.  The `content` is directly inspectable without any decoding step.
///
/// Metadata typically carries:
/// - `"mime"` — MIME type hint (`"text/plain"`, `"text/markdown"`, …)
/// - `"ext"`  — file extension hint (`"md"`, `"txt"`, …)
/// - `"ts"`   — ISO-8601 creation / modification timestamp
/// - `"role"` — for transcript entries: `"user"` / `"assistant"`
///
/// # Examples
/// ```rust
/// use araliya_bot::subsystems::memory::types::TextFile;
///
/// let mut tf = TextFile::new("# Hello\n".to_string());
/// tf.metadata.insert("mime".into(), "text/markdown".into());
/// assert_eq!(tf.mime(), "text/markdown");
/// assert!(!tf.is_empty());
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct TextFile {
    /// UTF-8 text payload.
    pub content: String,
    /// Free-form string metadata.
    pub metadata: HashMap<String, String>,
}

impl TextFile {
    /// Create a new `TextFile` with the given content and empty metadata.
    pub fn new(content: String) -> Self {
        Self { content, metadata: HashMap::new() }
    }

    /// Byte length of the content (`content.len()`).
    pub fn len(&self) -> usize { self.content.len() }

    /// `true` when the content is empty.
    pub fn is_empty(&self) -> bool { self.content.is_empty() }

    /// Byte slice of the content.
    pub fn as_bytes(&self) -> &[u8] { self.content.as_bytes() }

    /// MIME type from metadata, defaulting to `"text/plain"`.
    pub fn mime(&self) -> &str {
        self.metadata.get("mime").map(|s| s.as_str()).unwrap_or("text/plain")
    }
}

impl From<String> for TextFile { fn from(s: String) -> Self { Self::new(s) } }
impl From<&str>   for TextFile { fn from(s: &str)   -> Self { Self::new(s.to_string()) } }

// ── Value ─────────────────────────────────────────────────────────────────────

/// A value stored in a [`Block`](super::collections::Block) entry.
///
/// - `Primary` — cheap scalar (bool / int / float / str); comparable and hashable.
/// - `Text`    — human-readable UTF-8 content with metadata ([`TextFile`]).
/// - `Obj`     — opaque binary payload with metadata ([`Obj`]).
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Primary(PrimaryValue),
    /// A human-readable text document (Markdown, plain text, …).
    Text(TextFile),
    /// An opaque binary payload.
    Obj(Obj),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Primary(p) => write!(f, "{p}"),
            Self::Text(t)    => write!(f, "<Text {} bytes [{}]>", t.len(), t.mime()),
            Self::Obj(o)     => write!(f, "<Obj {} bytes>", o.len()),
        }
    }
}

impl From<PrimaryValue> for Value { fn from(p: PrimaryValue) -> Self { Self::Primary(p) } }
impl From<TextFile>     for Value { fn from(t: TextFile)     -> Self { Self::Text(t) } }
impl From<Obj>          for Value { fn from(o: Obj)          -> Self { Self::Obj(o) } }
impl From<bool>   for Value { fn from(v: bool)   -> Self { Self::Primary(PrimaryValue::Bool(v)) } }
impl From<i64>    for Value { fn from(v: i64)    -> Self { Self::Primary(PrimaryValue::Int(v)) } }
impl From<f64>    for Value { fn from(v: f64)    -> Self { Self::Primary(PrimaryValue::Float(v)) } }
impl From<String> for Value { fn from(v: String) -> Self { Self::Primary(PrimaryValue::Str(v)) } }
impl From<&str>   for Value { fn from(v: &str)   -> Self { Self::Primary(PrimaryValue::Str(v.to_string())) } }

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
    fn text_file_basics() {
        let tf = TextFile::new(String::new());
        assert!(tf.is_empty());
        let mut tf2 = TextFile::new("# Hello\nWorld".into());
        assert_eq!(tf2.len(), 13);
        assert!(!tf2.is_empty());
        assert_eq!(tf2.mime(), "text/plain");  // default
        tf2.metadata.insert("mime".into(), "text/markdown".into());
        assert_eq!(tf2.mime(), "text/markdown");
    }

    #[test]
    fn text_file_from_conversions() {
        let tf: TextFile = "raw".into();
        assert_eq!(tf.content, "raw");
        let tf2 = TextFile::from("hello".to_string());
        assert_eq!(tf2.content, "hello");
        assert!(tf2.metadata.is_empty());
    }

    #[test]
    fn value_display() {
        assert_eq!(Value::from(99i64).to_string(), "99");
        let o = Value::Obj(Obj::new(b"hello".to_vec()));
        assert_eq!(o.to_string(), "<Obj 5 bytes>");
        let mut tf = TextFile::new("hello".into());
        tf.metadata.insert("mime".into(), "text/markdown".into());
        let tv = Value::Text(tf);
        assert_eq!(tv.to_string(), "<Text 5 bytes [text/markdown]>");
    }

    #[test]
    fn value_from_conversions() {
        assert!(matches!(Value::from(true), Value::Primary(PrimaryValue::Bool(true))));
        assert!(matches!(Value::from(42i64), Value::Primary(PrimaryValue::Int(42))));
        assert!(matches!(Value::from("hi"), Value::Primary(PrimaryValue::Str(_))));
        assert!(matches!(Value::from(TextFile::new("doc".into())), Value::Text(_)));
        assert!(matches!(Value::from(Obj::new(vec![])), Value::Obj(_)));
    }
}
