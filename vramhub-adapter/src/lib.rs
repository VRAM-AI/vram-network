// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

//! # vramhub-adapter
//!
//! Training framework adapter trait for SLCL.
//!
//! SLCL is the primary system. Training frameworks (Templar, INTELLECT, custom)
//! plug into SLCL via this adapter, NOT the other way around.
//!
//! ## Design principle
//!
//! SLCL defines:
//! - What a gradient is (compressed tensor, R2 key format, content hash)
//! - What a checkpoint is (model weights, R2 key format, SHA256 hash)
//! - When training happens (window-based clock from Sui)
//! - How peers are scored (OpenSkill in TEE)
//! - How rewards are distributed (on-chain token emission)
//!
//! The training framework provides:
//! - How to produce a gradient (forward/backward pass, optimizer)
//! - How to load/save model state
//! - How to compress/decompress gradients
//! - Data loading and batching
//!
//! The adapter trait is the boundary between these two concerns.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use vramhub_adapter::TrainingFrameworkAdapter;
//!
//! // Miner startup
//! let adapter = TemplarAdapter::new(config)?; // or IntellectAdapter, or CustomAdapter
//! let miner = Miner::new(chain_config, adapter).await?;
//! miner.run().await?;
//! ```

#[cfg(feature = "candle")]
pub mod candle;
#[cfg(feature = "candle")]
pub mod candle_gpt;
pub mod custom;
#[cfg(feature = "intellect")]
pub mod intellect;
#[cfg(feature = "sidecar")]
pub mod sidecar;
#[cfg(feature = "templar")]
pub mod templar;
pub mod toy;
pub mod training;

use async_trait::async_trait;
use vramhub_core::{PeerId, VramhubError, WindowId};

/// A compressed gradient ready for upload to R2.
pub struct CompressedGradient {
    /// The compressed bytes (DCT + top-k + quantized)
    pub data: Vec<u8>,
    /// SHA256 hash of the compressed bytes
    pub content_hash: String,
    /// Size in bytes
    pub size_bytes: u64,
}

/// A model checkpoint.
pub struct Checkpoint {
    /// Serialized model weights
    pub data: Vec<u8>,
    /// SHA256 hash
    pub hash: [u8; 32],
}

/// Adapter trait for pluggable training frameworks.
///
/// Implementors provide the ML-specific logic. SLCL handles everything
/// else: registration, windowed scheduling, R2 upload/download, scoring,
/// and reward distribution.
#[async_trait]
pub trait TrainingFrameworkAdapter: Send + Sync {
    /// Human-readable name for logging.
    fn name(&self) -> &str;

    /// Initialize the model from a checkpoint.
    /// Called at startup and after each aggregation round.
    async fn load_checkpoint(&mut self, checkpoint_bytes: &[u8]) -> Result<(), VramhubError>;

    /// Run one training step on the given data batch.
    /// Returns raw (uncompressed) gradient tensors.
    async fn train_step(&mut self, batch: &[i64]) -> Result<Vec<f32>, VramhubError>;

    /// Compress a gradient for R2 upload.
    /// Must produce a deterministic output for the same input.
    fn compress_gradient(&self, raw_gradient: &[f32]) -> Result<CompressedGradient, VramhubError>;

    /// Decompress a gradient downloaded from R2.
    fn decompress_gradient(&self, compressed: &[u8]) -> Result<Vec<f32>, VramhubError>;

    /// Apply a gradient to the current model state (for aggregation).
    async fn apply_gradient(&mut self, gradient: &[f32], beta: f32) -> Result<(), VramhubError>;

    /// Serialize current model state as a checkpoint.
    async fn save_checkpoint(&self) -> Result<Checkpoint, VramhubError>;

    /// Get the data batch for a given (uid, window) pair.
    /// Must be deterministic: same (uid, window) always returns the same batch.
    fn get_assigned_batch(&self, uid: PeerId, window: WindowId) -> Vec<i64>;

    /// Get a random evaluation batch for a window.
    fn get_random_batch(&self, window: WindowId) -> Vec<i64>;

    /// Compute a forward pass loss on the current model.
    /// Used by the enclave evaluator.
    async fn forward_loss(&self, batch: &[i64]) -> Result<f32, VramhubError>;
}
