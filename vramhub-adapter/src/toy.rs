// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

//! Toy bigram language model adapter for local demo/testing.
//!
//! A minimal but real LLM: learns bigram transition probabilities over
//! a 10-word vocabulary. Loss decreases as the model trains, gradients
//! are real, and checkpoints are serializable — exercising the full
//! SLCL miner/validator/aggregator loop without heavy ML dependencies.
//!
//! Vocabulary (10 tokens):
//!   0:"the" 1:"cat" 2:"sat" 3:"on" 4:"mat" 5:"dog" 6:"ran" 7:"to" 8:"big" 9:"red"
//!
//! Model: 10×10 logits matrix W (100 f32 params).
//!   P(next=j | prev=i) = softmax(W[i])[j]
//!
//! Training: cross-entropy loss over bigram pairs in the batch.

use super::{Checkpoint, CompressedGradient, TrainingFrameworkAdapter};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use vramhub_core::{PeerId, VramhubError, WindowId};

pub const VOCAB_SIZE: usize = 10;
pub const NUM_PARAMS: usize = VOCAB_SIZE * VOCAB_SIZE; // 100
pub const VOCAB: [&str; VOCAB_SIZE] = [
    "the", "cat", "sat", "on", "mat", "dog", "ran", "to", "big", "red",
];

/// Toy bigram language model.
pub struct ToyAdapter {
    /// 10×10 logits matrix stored row-major. W[i*10 + j] = logit for j given i.
    weights: Vec<f32>,
    /// Learning rate
    lr: f32,
    /// Batch size for deterministic batch generation
    batch_size: usize,
}

impl ToyAdapter {
    pub fn new() -> Self {
        Self {
            weights: vec![0.0f32; NUM_PARAMS],
            lr: 0.1,
            batch_size: 32,
        }
    }

    pub fn with_lr(mut self, lr: f32) -> Self {
        self.lr = lr;
        self
    }
}

impl Default for ToyAdapter {
    fn default() -> Self {
        Self::new()
    }
}

/// Softmax over a slice, returns probabilities.
fn softmax(logits: &[f32]) -> Vec<f32> {
    let max = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let exps: Vec<f32> = logits.iter().map(|&x| (x - max).exp()).collect();
    let sum: f32 = exps.iter().sum();
    exps.iter().map(|&e| e / sum).collect()
}

/// Convert a batch of i64 values to token IDs in [0, VOCAB_SIZE).
fn to_token_ids(batch: &[i64]) -> Vec<usize> {
    batch
        .iter()
        .map(|&v| (v.unsigned_abs() % VOCAB_SIZE as u64) as usize)
        .collect()
}

/// Compute cross-entropy loss over bigram pairs in a token sequence.
pub fn bigram_loss(weights: &[f32], tokens: &[usize]) -> f32 {
    if tokens.len() < 2 {
        return 0.0;
    }
    let mut total_loss = 0.0f32;
    let pairs = tokens.len() - 1;
    for i in 0..pairs {
        let prev = tokens[i];
        let next = tokens[i + 1];
        let row = &weights[prev * VOCAB_SIZE..(prev + 1) * VOCAB_SIZE];
        let probs = softmax(row);
        total_loss -= probs[next].max(1e-10).ln();
    }
    total_loss / pairs as f32
}

/// Compute gradient of cross-entropy loss w.r.t. the weight matrix.
fn bigram_gradient(weights: &[f32], tokens: &[usize]) -> Vec<f32> {
    let mut grad = vec![0.0f32; NUM_PARAMS];
    if tokens.len() < 2 {
        return grad;
    }
    let pairs = tokens.len() - 1;
    for i in 0..pairs {
        let prev = tokens[i];
        let next = tokens[i + 1];
        let row = &weights[prev * VOCAB_SIZE..(prev + 1) * VOCAB_SIZE];
        let probs = softmax(row);
        for j in 0..VOCAB_SIZE {
            // d(loss)/d(W[prev][j]) = probs[j] - 1{j == next}
            let target = if j == next { 1.0 } else { 0.0 };
            grad[prev * VOCAB_SIZE + j] += (probs[j] - target) / pairs as f32;
        }
    }
    grad
}

/// Deterministic batch generation from (uid, window) seed.
fn deterministic_batch(uid: PeerId, window: WindowId, size: usize) -> Vec<i64> {
    let seed = uid ^ window;
    let mut state = seed;
    (0..size)
        .map(|_| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            state as i64
        })
        .collect()
}

/// Serialize f32 slice to bytes (little-endian).
pub fn f32_to_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for &f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

/// Deserialize bytes to f32 vec (little-endian).
pub fn bytes_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

#[async_trait]
impl TrainingFrameworkAdapter for ToyAdapter {
    fn name(&self) -> &str {
        "toy-bigram"
    }

    async fn load_checkpoint(&mut self, checkpoint_bytes: &[u8]) -> Result<(), VramhubError> {
        if checkpoint_bytes.is_empty() {
            self.weights = vec![0.0f32; NUM_PARAMS];
            return Ok(());
        }
        if checkpoint_bytes.len() != NUM_PARAMS * 4 {
            return Err(VramhubError::Internal(format!(
                "toy checkpoint: expected {} bytes, got {}",
                NUM_PARAMS * 4,
                checkpoint_bytes.len()
            )));
        }
        self.weights = bytes_to_f32(checkpoint_bytes);
        Ok(())
    }

    async fn train_step(&mut self, batch: &[i64]) -> Result<Vec<f32>, VramhubError> {
        let tokens = to_token_ids(batch);
        let grad = bigram_gradient(&self.weights, &tokens);
        // Apply gradient locally (SGD step)
        for (w, g) in self.weights.iter_mut().zip(grad.iter()) {
            *w -= self.lr * g;
        }
        Ok(grad)
    }

    fn compress_gradient(&self, raw: &[f32]) -> Result<CompressedGradient, VramhubError> {
        let data = f32_to_bytes(raw);
        let hash = hex::encode(Sha256::digest(&data));
        let size = data.len() as u64;
        Ok(CompressedGradient {
            data,
            content_hash: hash,
            size_bytes: size,
        })
    }

    fn decompress_gradient(&self, compressed: &[u8]) -> Result<Vec<f32>, VramhubError> {
        Ok(bytes_to_f32(compressed))
    }

    async fn apply_gradient(&mut self, gradient: &[f32], beta: f32) -> Result<(), VramhubError> {
        for (w, g) in self.weights.iter_mut().zip(gradient.iter()) {
            *w -= beta * g;
        }
        Ok(())
    }

    async fn save_checkpoint(&self) -> Result<Checkpoint, VramhubError> {
        let data = f32_to_bytes(&self.weights);
        let hash: [u8; 32] = Sha256::digest(&data).into();
        Ok(Checkpoint { data, hash })
    }

    fn get_assigned_batch(&self, uid: PeerId, window: WindowId) -> Vec<i64> {
        deterministic_batch(uid, window, self.batch_size)
    }

    fn get_random_batch(&self, window: WindowId) -> Vec<i64> {
        deterministic_batch(window, 0, self.batch_size)
    }

    async fn forward_loss(&self, batch: &[i64]) -> Result<f32, VramhubError> {
        let tokens = to_token_ids(batch);
        Ok(bigram_loss(&self.weights, &tokens))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn softmax_sums_to_one() {
        let logits = vec![1.0, 2.0, 3.0, 0.5, -1.0, 0.0, 1.5, 2.5, -0.5, 0.1];
        let probs = softmax(&logits);
        let sum: f32 = probs.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5);
    }

    #[test]
    fn to_token_ids_wraps_correctly() {
        let batch = vec![10i64, -3, 25, 0, 9];
        let ids = to_token_ids(&batch);
        assert!(ids.iter().all(|&id| id < VOCAB_SIZE));
    }

    #[test]
    fn initial_loss_is_log_vocab() {
        let w = vec![0.0f32; NUM_PARAMS];
        let tokens: Vec<usize> = (0..20).map(|i| i % VOCAB_SIZE).collect();
        let loss = bigram_loss(&w, &tokens);
        let expected = (VOCAB_SIZE as f32).ln();
        assert!(
            (loss - expected).abs() < 0.1,
            "initial loss ~= ln(V), got {loss}"
        );
    }

    #[tokio::test]
    async fn training_reduces_loss() {
        let mut adapter = ToyAdapter::new().with_lr(0.5);
        let batch: Vec<i64> = (0..100).collect();
        let loss_before = adapter.forward_loss(&batch).await.unwrap();
        for _ in 0..50 {
            adapter.train_step(&batch).await.unwrap();
        }
        let loss_after = adapter.forward_loss(&batch).await.unwrap();
        assert!(
            loss_after < loss_before,
            "loss should decrease: {loss_before} -> {loss_after}"
        );
    }

    #[tokio::test]
    async fn checkpoint_round_trip() {
        let mut adapter = ToyAdapter::new();
        let batch: Vec<i64> = (0..20).collect();
        adapter.train_step(&batch).await.unwrap();
        let ckpt = adapter.save_checkpoint().await.unwrap();
        let mut adapter2 = ToyAdapter::new();
        adapter2.load_checkpoint(&ckpt.data).await.unwrap();
        assert_eq!(adapter.weights, adapter2.weights);
    }

    #[test]
    fn deterministic_batch_is_reproducible() {
        let a = deterministic_batch(1, 42, 10);
        let b = deterministic_batch(1, 42, 10);
        assert_eq!(a, b);
    }

    #[test]
    fn gradient_has_correct_size() {
        let w = vec![0.0f32; NUM_PARAMS];
        let tokens: Vec<usize> = vec![0, 1, 2, 3, 4];
        let grad = bigram_gradient(&w, &tokens);
        assert_eq!(grad.len(), NUM_PARAMS);
    }
}
