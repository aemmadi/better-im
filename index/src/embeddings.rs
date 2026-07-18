//! On-device text embeddings for semantic search (Phase 5).
//!
//! The rest of Phase 5 (vector storage, KNN, hybrid ranking) is written against
//! the [`Embedder`] trait so it never depends on a particular model — and so the
//! whole crate builds and tests **without a network connection or a real model**.
//!
//! Two implementations are provided:
//!
//! - [`MockEmbedder`] — deterministic, dependency-free pseudo-embeddings (hashed
//!   bag of words + character n-grams → an L2-normalized vector). Texts that
//!   share vocabulary land near each other in cosine space, which is enough to
//!   exercise nearest-neighbour, fusion, and backfill logic. **Every test uses
//!   it**, so the suite is offline and reproducible.
//! - [`FastEmbedEmbedder`] — the production model (`BAAI/bge-small-en-v1.5` via
//!   the `fastembed` crate), **feature-gated behind `fastembed`** so the default
//!   workspace build never pulls in the `ort`/onnxruntime native stack. The model
//!   downloads on first use on the user's machine.

use anyhow::Result;

/// Produces fixed-width embedding vectors for a batch of texts.
///
/// Implementations must be cheap to clone-share (`Send + Sync`) because a single
/// embedder is held behind an `Arc` and used from blocking worker threads.
pub trait Embedder: Send + Sync {
    /// Embed a batch of texts, returning one vector per input (same order).
    /// Every returned vector has length [`dim`](Self::dim).
    ///
    /// # Errors
    /// Returns an error if the underlying model fails to run.
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;

    /// Dimensionality of the vectors this embedder produces.
    fn dim(&self) -> usize;

    /// A stable identifier for the producing model (e.g. `"mock-v1/64"` or
    /// `"bge-small-en-v1.5"`). Stored alongside each vector so a later model swap
    /// is detectable and the index can be rebuilt.
    fn model_tag(&self) -> String;
}

/// Deterministic, offline pseudo-embedder used by all tests (and as the default
/// build's stand-in when the `fastembed` feature is off).
///
/// The mapping is a classic hashing trick: every lowercase word and every
/// character 3-gram of that word is hashed (stable FNV-1a) into one of `dim`
/// buckets and accumulated; the resulting vector is L2-normalized. Two texts that
/// share words/sub-words therefore share buckets and score a high cosine
/// similarity — the property nearest-neighbour search relies on — while the
/// output is fully reproducible across platforms and Rust versions.
#[derive(Debug, Clone)]
pub struct MockEmbedder {
    dim: usize,
}

impl MockEmbedder {
    /// Create a mock embedder producing `dim`-dimensional vectors. `dim` must be
    /// non-zero; it is clamped to at least 1.
    #[must_use]
    pub fn new(dim: usize) -> Self {
        Self { dim: dim.max(1) }
    }

    /// Embed a single text into a normalized vector.
    #[must_use]
    pub fn embed_one(&self, text: &str) -> Vec<f32> {
        let mut v = vec![0.0f32; self.dim];
        for word in tokenize(text) {
            // Whole-word feature (weighted higher than sub-word features).
            add_feature(&mut v, &fnv1a(word.as_bytes()), 2.0);
            // Character 3-grams capture morphological overlap (dinner/dinners).
            let chars: Vec<char> = word.chars().collect();
            if chars.len() >= 3 {
                for w in chars.windows(3) {
                    let gram: String = w.iter().collect();
                    add_feature(&mut v, &fnv1a(gram.as_bytes()), 1.0);
                }
            }
        }
        normalize(&mut v);
        v
    }
}

impl Default for MockEmbedder {
    fn default() -> Self {
        Self::new(64)
    }
}

impl Embedder for MockEmbedder {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| self.embed_one(t)).collect())
    }

    fn dim(&self) -> usize {
        self.dim
    }

    fn model_tag(&self) -> String {
        format!("mock-v1/{}", self.dim)
    }
}

/// Split into lowercase alphanumeric word tokens.
fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Fold a 64-bit hash into a bucket + sign and accumulate `weight` there. The
/// sign bit decorrelates collisions so distinct features do not merely pile up.
fn add_feature(v: &mut [f32], hash: &u64, weight: f32) {
    let dim = v.len() as u64;
    let bucket = (hash % dim) as usize;
    let sign = if (hash >> 63) & 1 == 1 { 1.0 } else { -1.0 };
    v[bucket] += weight * sign;
}

/// In-place L2 normalization (no-op on the zero vector).
fn normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > f32::EPSILON {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

/// Stable FNV-1a 64-bit hash (fixed constants -> reproducible everywhere).
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Cosine similarity of two equal-length vectors, in `[-1, 1]`. Returns `0.0`
/// when either vector is zero-length or all-zero, and when the lengths differ.
#[must_use]
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    let denom = na.sqrt() * nb.sqrt();
    if denom > f32::EPSILON {
        dot / denom
    } else {
        0.0
    }
}

/// Encode a vector as a little-endian `f32` byte blob for storage.
#[must_use]
pub fn encode_vector(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

/// Decode a little-endian `f32` byte blob back into a vector. Trailing bytes that
/// do not form a whole `f32` are ignored.
#[must_use]
pub fn decode_vector(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Production embedder: `BAAI/bge-small-en-v1.5` (384-dim) via `fastembed`.
///
/// Feature-gated behind `fastembed` so the default build never links the
/// `ort`/onnxruntime native stack. The ONNX model + tokenizer download on first
/// `embed` call (cached under the fastembed cache dir), so construction is cheap
/// and offline; only the first embedding needs the network, on the user's Mac.
#[cfg(feature = "fastembed")]
pub struct FastEmbedEmbedder {
    model: std::sync::Mutex<Option<fastembed::TextEmbedding>>,
}

#[cfg(feature = "fastembed")]
impl FastEmbedEmbedder {
    /// bge-small-en-v1.5 output dimensionality.
    pub const DIM: usize = 384;

    /// Create the embedder without loading the model (loaded lazily on first
    /// [`embed`](Embedder::embed)).
    #[must_use]
    pub fn new() -> Self {
        Self {
            model: std::sync::Mutex::new(None),
        }
    }
}

#[cfg(feature = "fastembed")]
impl Default for FastEmbedEmbedder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "fastembed")]
impl Embedder for FastEmbedEmbedder {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

        let mut guard = self.model.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        if guard.is_none() {
            // Downloads + caches the model on first use (needs network once).
            let model = TextEmbedding::try_new(
                InitOptions::new(EmbeddingModel::BGESmallENV15).with_show_download_progress(false),
            )?;
            *guard = Some(model);
        }
        let model = guard.as_ref().expect("model initialized above");
        // fastembed returns L2-normalized vectors for this model.
        let out = model.embed(texts.to_vec(), None)?;
        Ok(out)
    }

    fn dim(&self) -> usize {
        Self::DIM
    }

    fn model_tag(&self) -> String {
        "bge-small-en-v1.5".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_is_deterministic_and_normalized() {
        let e = MockEmbedder::new(64);
        let a = e.embed_one("let's grab dinner tonight");
        let b = e.embed_one("let's grab dinner tonight");
        assert_eq!(a, b, "same text -> identical vector");
        assert_eq!(a.len(), 64);
        let norm: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5, "L2-normalized (norm={norm})");
    }

    #[test]
    fn related_texts_are_nearer_than_unrelated() {
        let e = MockEmbedder::new(128);
        let q = e.embed_one("what time is dinner");
        let related = e.embed_one("dinner is at eight");
        let unrelated = e.embed_one("the quarterly budget spreadsheet");
        let s_rel = cosine_similarity(&q, &related);
        let s_unrel = cosine_similarity(&q, &unrelated);
        assert!(
            s_rel > s_unrel,
            "shared-vocabulary text should be nearer: {s_rel} vs {s_unrel}"
        );
    }

    #[test]
    fn vector_blob_roundtrips() {
        let v = vec![1.0f32, -2.5, 0.0, 42.5];
        let bytes = encode_vector(&v);
        assert_eq!(bytes.len(), 16);
        assert_eq!(decode_vector(&bytes), v);
    }

    #[test]
    fn batch_embed_matches_single() {
        let e = MockEmbedder::new(32);
        let texts = vec!["hello world".to_string(), "goodbye moon".to_string()];
        let batch = e.embed(&texts).unwrap();
        assert_eq!(batch.len(), 2);
        assert_eq!(batch[0], e.embed_one("hello world"));
        assert_eq!(batch[1], e.embed_one("goodbye moon"));
    }
}
