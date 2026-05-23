//! Local semantic memory: offline search over captured text. See `Plans.md`
//! (task 7.5).
//!
//! [`SemanticIndex`] holds `(id, vector)` entries and ranks a query by cosine
//! similarity — entirely offline, no service. Vectors come from an [`Embedder`];
//! the built-in [`HashingEmbedder`] is a dependency-free, deterministic *lexical*
//! embedder (feature-hashed term frequencies), so out of the box "semantic"
//! search is really strong lexical search. For true semantic similarity, plug a
//! neural model by implementing [`Embedder`] — the same bring-your-own-model
//! pattern as [`crate::structured::LlmClient`].
//!
//! Callers index whatever captured text they want searchable (e.g. a snapshot's
//! readable text keyed by URL); the engine embeds nothing automatically.

/// Turns text into a fixed-length vector. Implement this for a neural model to
/// get true semantic embeddings; the default [`HashingEmbedder`] is lexical.
pub trait Embedder {
    /// Embed `text` into a vector. All vectors from one embedder must share a
    /// length so they can be compared.
    fn embed(&self, text: &str) -> Vec<f32>;
}

/// Dependency-free, deterministic lexical embedder: tokenizes `text`, feature-
/// hashes each token into one of `dim` buckets (FNV-1a), counts term
/// frequencies, and L2-normalizes. Lexical, not neural — see the module docs.
#[derive(Debug, Clone)]
pub struct HashingEmbedder {
    dim: usize,
}

impl Default for HashingEmbedder {
    fn default() -> Self {
        Self::new(256)
    }
}

impl HashingEmbedder {
    /// A hashing embedder producing `dim`-dimensional vectors (`dim` clamped to
    /// at least 1).
    pub fn new(dim: usize) -> Self {
        Self { dim: dim.max(1) }
    }
}

impl Embedder for HashingEmbedder {
    fn embed(&self, text: &str) -> Vec<f32> {
        let mut v = vec![0.0f32; self.dim];
        for token in tokenize(text) {
            v[(fnv1a(&token) % self.dim as u64) as usize] += 1.0;
        }
        l2_normalize(&mut v);
        v
    }
}

/// Lowercase alphanumeric tokens.
fn tokenize(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
}

/// FNV-1a hash — small and stable across runs (so embeddings are reproducible).
fn fnv1a(s: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Scale `v` to unit length in place (a no-op for the zero vector).
fn l2_normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

/// Cosine similarity of two equal-length vectors, in `[-1, 1]` (0 if either is
/// zero or lengths differ).
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na > 0.0 && nb > 0.0 {
        dot / (na * nb)
    } else {
        0.0
    }
}

/// An offline, cosine-ranked index over embedded text.
pub struct SemanticIndex<E: Embedder> {
    embedder: E,
    entries: Vec<(String, Vec<f32>)>,
}

impl<E: Embedder> SemanticIndex<E> {
    /// Build an empty index backed by `embedder`.
    pub fn new(embedder: E) -> Self {
        Self {
            embedder,
            entries: Vec::new(),
        }
    }

    /// Index `text` under `id` (e.g. a captured page's readable text by URL).
    pub fn add(&mut self, id: impl Into<String>, text: &str) {
        let vector = self.embedder.embed(text);
        self.entries.push((id.into(), vector));
    }

    /// Number of indexed entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The `k` indexed entries most similar to `query`, as `(id, score)` sorted
    /// by descending cosine similarity.
    pub fn search(&self, query: &str, k: usize) -> Vec<(String, f32)> {
        let q = self.embedder.embed(query);
        let mut scored: Vec<(String, f32)> = self
            .entries
            .iter()
            .map(|(id, v)| (id.clone(), cosine_similarity(&q, v)))
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        scored.truncate(k);
        scored
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedding_is_deterministic_and_normalized() {
        let e = HashingEmbedder::default();
        let a = e.embed("the quick brown fox");
        let b = e.embed("the quick brown fox");
        assert_eq!(a, b, "same text must embed identically (reproducible)");
        let norm = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-5,
            "non-empty text embeds to unit length"
        );
    }

    #[test]
    fn empty_text_embeds_to_zero_vector() {
        let v = HashingEmbedder::new(16).embed("   !!!  ");
        assert!(v.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn cosine_basics() {
        let a = [1.0, 0.0, 0.0];
        let b = [0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &a) - 1.0).abs() < 1e-6);
        assert!(cosine_similarity(&a, &b).abs() < 1e-6); // orthogonal
        assert_eq!(cosine_similarity(&a, &[1.0, 0.0]), 0.0); // length mismatch
        assert_eq!(cosine_similarity(&[0.0, 0.0], &b), 0.0); // zero vector
    }

    #[test]
    fn search_ranks_the_lexically_relevant_doc_first() {
        let mut index = SemanticIndex::new(HashingEmbedder::default());
        index.add("rust", "the rust programming language is fast and safe");
        index.add("cooking", "a recipe for baking sourdough bread at home");
        index.add("gardening", "how to grow tomatoes and peppers in a garden");

        let hits = index.search("safe systems programming in rust", 3);
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0].0, "rust", "the rust doc should rank first");
        assert!(hits[0].1 > hits[1].1, "top score should beat the rest");
    }

    #[test]
    fn search_k_limits_and_empty_index_is_safe() {
        let mut index = SemanticIndex::new(HashingEmbedder::default());
        assert!(index.is_empty());
        assert!(index.search("anything", 5).is_empty());

        index.add("a", "alpha beta gamma");
        index.add("b", "delta epsilon zeta");
        assert_eq!(index.len(), 2);
        assert_eq!(index.search("alpha", 1).len(), 1);
    }
}
