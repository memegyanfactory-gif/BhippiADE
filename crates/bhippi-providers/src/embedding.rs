//! Deterministic, dependency-free semantic embedding (Phase B5).
//!
//! Unlike an ML model, this performs no neural inference and downloads nothing:
//! it feature-hashes tokenised text into a fixed-size, L2-normalised vector so
//! cosine similarity is a cheap, reproducible proxy for text overlap.  Results
//! are stable across runs and machines — the same input always yields the same
//! vector — which makes it safe to store in the Project Brain and to detect
//! staleness when the model version changes.
//!
//! This is intentionally the *least* sophisticated model: semantic search is one
//! retrieval signal, not the entire brain (plan SEC. 5).  Exact-name matching and
//! structural lookup always beat it in `ProjectBrain::search`.

/// Identifier for the current embedding model/version.  Stored alongside vectors
/// so a model bump triggers a re-embed rather than silently mixing feature spaces.
pub const EMBEDDING_MODEL: &str = "bhippi-token-hash-v1";

/// Dimensionality of the feature space.  Compact by design — it is a hashing
/// output, not a learned embedding, so larger dimensions add little.
pub const EMBEDDING_DIM: usize = 256;

/// A deterministic document embedding.
#[derive(Clone, Debug, PartialEq)]
pub struct Embedding {
    /// Model identifier used to build this vector (see [`EMBEDDING_MODEL`]).
    pub model: String,
    /// Dimensionality of `values`; always [`EMBEDDING_DIM`] for this model.
    pub dim: usize,
    /// Dense, L2-normalised feature vector.
    pub values: Vec<f32>,
}

/// A small stable hash for a token (FNV-1a, 64-bit).
///
/// We implement it manually rather than relying on `DefaultHasher` so vectors are
/// stable across Rust versions and platforms, matching the "reproducible" promise.
fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Tokenise `text` into normalised words: lowercased, alphanumeric-only, length ≥ 2.
fn tokens(text: &str) -> Vec<String> {
    let lower = text.to_ascii_lowercase();
    let alnum: String = lower
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    alnum
        .split_whitespace()
        .map(str::to_owned)
        .filter(|t| t.len() >= 2)
        .collect()
}

/// Build a deterministic unit vector for `text` over a hash bucket feature space.
#[must_use]
pub fn embed(text: &str) -> Embedding {
    let mut counts = vec![0_u32; EMBEDDING_DIM];
    for token in tokens(text) {
        let bucket = (fnv1a_64(token.as_bytes()) % EMBEDDING_DIM as u64) as usize;
        counts[bucket] = counts[bucket].saturating_add(1);
    }

    let denom = counts
        .iter()
        .map(|&c| u64::from(c) * u64::from(c))
        .sum::<u64>();
    if denom == 0 {
        return Embedding {
            model: EMBEDDING_MODEL.to_owned(),
            dim: EMBEDDING_DIM,
            values: vec![0.0; EMBEDDING_DIM],
        };
    }
    let norm = (denom as f64).sqrt() as f32;
    let values: Vec<f32> = counts.iter().map(|&c| c as f32 / norm).collect();

    Embedding {
        model: EMBEDDING_MODEL.to_owned(),
        dim: EMBEDDING_DIM,
        values,
    }
}

/// Cosine similarity between two vectors of equal length.  Returns `None` if the
/// lengths differ or either is entirely zero.
#[must_use]
pub fn cosine(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.len() != b.len() || a.is_empty() {
        return None;
    }
    let mut dot = 0.0_f64;
    let mut a_sq = 0.0_f64;
    let mut b_sq = 0.0_f64;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += f64::from(*x) * f64::from(*y);
        a_sq += f64::from(*x) * f64::from(*x);
        b_sq += f64::from(*y) * f64::from(*y);
    }
    let a_norm = a_sq.sqrt();
    let b_norm = b_sq.sqrt();
    if a_norm == 0.0 || b_norm == 0.0 {
        return None;
    }
    Some((dot / (a_norm * b_norm)) as f32)
}

/// Encode an [`Embedding`] into a byte blob for storage in `brain_symbols.embedding_blob`.
///
/// Layout: `[model_len: u16][model: bytes][dim: u32][values: f32 LE × dim]`.
#[must_use]
pub fn encode(embedding: &Embedding) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + embedding.model.len() + 4 + embedding.dim * 4);
    out.extend_from_slice(&(embedding.model.len() as u16).to_le_bytes());
    out.extend_from_slice(embedding.model.as_bytes());
    out.extend_from_slice(&(embedding.dim as u32).to_le_bytes());
    for v in &embedding.values {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Decode a byte blob produced by [`encode`].  Returns `None` for malformed input
/// or a mismatch between the recorded dimension and the value count.
#[must_use]
pub fn decode(blob: &[u8]) -> Option<Embedding> {
    if blob.len() < 2 {
        return None;
    }
    let mut offset = 0usize;
    let model_len = u16::from_le_bytes([blob[0], blob[1]]) as usize;
    offset += 2;
    if blob.len() < offset + model_len + 4 {
        return None;
    }
    let model_bytes = &blob[offset..offset + model_len];
    offset += model_len;
    let dim_bytes = [
        blob[offset],
        blob[offset + 1],
        blob[offset + 2],
        blob[offset + 3],
    ];
    let dim = u32::from_le_bytes(dim_bytes) as usize;
    offset += 4;
    if blob.len() != offset + dim * 4 {
        return None;
    }
    let mut values = Vec::with_capacity(dim);
    for chunk in blob[offset..].chunks_exact(4) {
        let arr: [u8; 4] = [chunk[0], chunk[1], chunk[2], chunk[3]];
        values.push(f32::from_le_bytes(arr));
    }
    Some(Embedding {
        model: String::from_utf8_lossy(model_bytes).into_owned(),
        dim,
        values,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embed_is_deterministic() {
        let a = embed("fn greet_user(name) -> String");
        let b = embed("fn greet_user(name) -> String");
        assert_eq!(a, b);
        assert_eq!(a.model, EMBEDDING_MODEL);
        assert_eq!(a.dim, EMBEDDING_DIM);
        assert_eq!(a.values.len(), EMBEDDING_DIM);
    }

    #[test]
    fn similar_text_scores_higher_than_unrelated() {
        let movement = embed("where is player movement processed render update");
        let similar = embed("player movement processing update rendering");
        let unrelated = embed("database connection pool auth login token");
        let sim = cosine(&movement.values, &similar.values);
        let unrel = cosine(&movement.values, &unrelated.values);
        let (sim, unrel) = (sim.unwrap_or(f32::MIN), unrel.unwrap_or(f32::MIN));
        assert!(
            sim > unrel,
            "similar ({sim}) should beat unrelated ({unrel})"
        );
    }

    #[test]
    fn identical_vectors_have_unit_similarity() {
        let e = embed("some stable sentence");
        let similarity = cosine(&e.values, &e.values);
        assert!(similarity.is_some());
        let v = similarity.unwrap_or(0.0);
        assert!(
            (v - 1.0).abs() < 1e-5,
            "self-similarity should be ~1, got {v}"
        );
    }

    #[test]
    fn encode_decode_round_trips() {
        let e = embed("a round trip for storage");
        let blob = encode(&e);
        let decoded = decode(&blob).expect("decode should succeed");
        assert_eq!(decoded, e);
    }

    #[test]
    fn decode_rejects_truncated_blob() {
        let e = embed("truncation check");
        let blob = encode(&e);
        assert!(decode(&blob[..blob.len() - 4]).is_none());
    }
}
