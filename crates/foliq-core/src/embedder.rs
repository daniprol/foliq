//! Embedders: turn texts into vectors, for semantic search.
//!
//! foliq 0.1 searches with keywords (BM25) only. This interface is here so
//! plugins can be written against it now; the engine starts calling it in 0.2.

use crate::Result;

/// Turns texts into vectors of numbers.
pub trait Embedder: Send + Sync {
    /// Changes whenever vectors would change (the model, its dimensions...).
    fn fingerprint(&self) -> String;

    /// The length of every vector.
    fn dimensions(&self) -> usize;

    /// Embeds a batch of texts. Returns one vector per text, in order.
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
}
