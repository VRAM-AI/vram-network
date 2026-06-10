// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

//! Minimal example / testnet stub adapter.
//!
//! Produces synthetic gradients so `vramhub-miner` can participate in the
//! network without real ML hardware.  Replace the implementations below with
//! your actual training logic for a production miner.

use super::{Checkpoint, CompressedGradient, TrainingFrameworkAdapter};
use async_trait::async_trait;
use vramhub_core::{PeerId, VramhubError, WindowId};

pub struct CustomAdapter {
    // Replace with your model state
    dummy_weights: Vec<f32>,
}

impl CustomAdapter {
    pub fn new() -> Self {
        // 1024 synthetic weights, all near zero
        Self {
            dummy_weights: vec![0.0f32; 1024],
        }
    }
}

impl Default for CustomAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TrainingFrameworkAdapter for CustomAdapter {
    fn name(&self) -> &str {
        "custom (stub)"
    }

    async fn load_checkpoint(&mut self, checkpoint_bytes: &[u8]) -> Result<(), VramhubError> {
        // Stub: treat checkpoint bytes as raw f32 weights if non-empty
        if checkpoint_bytes.len() >= 4 {
            let floats: Vec<f32> = checkpoint_bytes
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect();
            self.dummy_weights = floats;
        }
        Ok(())
    }

    /// Produces a synthetic gradient deterministically derived from the batch tokens.
    /// Replace with a real forward+backward pass for production.
    async fn train_step(&mut self, batch: &[i64]) -> Result<Vec<f32>, VramhubError> {
        let n = self.dummy_weights.len().max(batch.len()).max(1);
        let batch_len = batch.len().max(1);
        // Compute gradient without borrowing self.dummy_weights mutably inside the closure
        let gradient: Vec<f32> = (0..n)
            .map(|i| {
                let token = batch.get(i % batch_len).copied().unwrap_or(0);
                (token as f32 * 0.0001) * ((i as f32 * 0.01).sin())
            })
            .collect();
        // Apply gradient to weights separately
        let wlen = self.dummy_weights.len();
        for (i, g) in gradient.iter().enumerate() {
            self.dummy_weights[i % wlen] -= 0.001 * g;
        }
        Ok(gradient)
    }

    fn compress_gradient(&self, _raw: &[f32]) -> Result<CompressedGradient, VramhubError> {
        todo!("DCT + top-k compress")
    }

    fn decompress_gradient(&self, _compressed: &[u8]) -> Result<Vec<f32>, VramhubError> {
        todo!("Inverse top-k + DCT decompress")
    }

    async fn apply_gradient(&mut self, gradient: &[f32], beta: f32) -> Result<(), VramhubError> {
        for (w, g) in self.dummy_weights.iter_mut().zip(gradient.iter()) {
            *w -= beta * g;
        }
        Ok(())
    }

    async fn save_checkpoint(&self) -> Result<Checkpoint, VramhubError> {
        use sha2::{Digest, Sha256};
        let data: Vec<u8> = self
            .dummy_weights
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        let hash: [u8; 32] = Sha256::digest(&data).into();
        Ok(Checkpoint { data, hash })
    }

    fn get_assigned_batch(&self, uid: PeerId, window: WindowId) -> Vec<i64> {
        // Deterministic batch stub
        (0..64).map(|i| uid as i64 ^ (window as i64 + i)).collect()
    }

    fn get_random_batch(&self, window: WindowId) -> Vec<i64> {
        (0..64).map(|i| window as i64 + i).collect()
    }

    async fn forward_loss(&self, batch: &[i64]) -> Result<f32, VramhubError> {
        // Stub loss: sum of batch / norm
        let loss = batch.iter().map(|&t| (t as f32).abs()).sum::<f32>() / batch.len().max(1) as f32;
        Ok(loss)
    }
}
