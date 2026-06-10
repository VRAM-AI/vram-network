// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

//! Templar training framework adapter.
//!
//! Wraps Templar's existing Python training code via PyO3 or subprocess.
//!
//! NOTE: This adapter exists so that teams already using Templar can
//! migrate to SLCL incrementally. New projects should implement
//! TrainingFrameworkAdapter directly in Rust for better performance.

use super::{Checkpoint, CompressedGradient, TrainingFrameworkAdapter};
#[cfg(feature = "templar")]
use async_trait::async_trait;
use vramhub_core::{PeerId, VramhubError, WindowId};

/// Adapter that delegates to Templar's Python training code.
///
/// Communicates via a local subprocess or PyO3 bridge.
/// The Python side must expose:
/// - load_checkpoint(bytes) -> None
/// - train_step(batch) -> gradient_tensor
/// - compress_gradient(tensor) -> bytes
/// - decompress_gradient(bytes) -> tensor
/// - apply_gradient(tensor, beta) -> None
/// - save_checkpoint() -> bytes
/// - forward_loss(batch) -> float
pub struct TemplarAdapter {
    // PyO3 interpreter handle or subprocess handle
}

impl TemplarAdapter {
    pub fn new(_templar_config_path: &str) -> Result<Self, VramhubError> {
        todo!("Initialize PyO3 interpreter with Templar modules loaded")
    }
}

#[cfg(feature = "templar")]
#[async_trait]
impl TrainingFrameworkAdapter for TemplarAdapter {
    fn name(&self) -> &str {
        "templar"
    }

    async fn load_checkpoint(&mut self, _bytes: &[u8]) -> Result<(), VramhubError> {
        todo!("Call Python: templar.load_checkpoint(bytes)")
    }

    async fn train_step(&mut self, _batch: &[i64]) -> Result<Vec<f32>, VramhubError> {
        todo!("Call Python: templar.train_step(batch)")
    }

    fn compress_gradient(&self, _raw: &[f32]) -> Result<CompressedGradient, VramhubError> {
        todo!("Call Python: templar.compress_gradient(tensor)")
    }

    fn decompress_gradient(&self, _compressed: &[u8]) -> Result<Vec<f32>, VramhubError> {
        todo!("Call Python: templar.decompress_gradient(bytes)")
    }

    async fn apply_gradient(&mut self, _gradient: &[f32], _beta: f32) -> Result<(), VramhubError> {
        todo!("Call Python: templar.apply_gradient(tensor, beta)")
    }

    async fn save_checkpoint(&self) -> Result<Checkpoint, VramhubError> {
        todo!("Call Python: templar.save_checkpoint()")
    }

    fn get_assigned_batch(&self, _uid: PeerId, _window: WindowId) -> Vec<i64> {
        todo!("Call Python: templar.get_assigned_batch(uid, window)")
    }

    fn get_random_batch(&self, _window: WindowId) -> Vec<i64> {
        todo!("Call Python: templar.get_random_batch(window)")
    }

    async fn forward_loss(&self, _batch: &[i64]) -> Result<f32, VramhubError> {
        todo!("Call Python: templar.forward_loss(batch)")
    }
}
