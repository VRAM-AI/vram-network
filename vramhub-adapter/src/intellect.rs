// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

//! INTELLECT training framework adapter.
//!
//! Stub implementation. See custom.rs for the minimal adapter skeleton
//! and templar.rs for the PyO3/subprocess bridge pattern.

use super::{Checkpoint, CompressedGradient, TrainingFrameworkAdapter};
#[cfg(feature = "intellect")]
use async_trait::async_trait;
use vramhub_core::{PeerId, VramhubError, WindowId};

pub struct IntellectAdapter {}

impl IntellectAdapter {
    pub fn new(_config_path: &str) -> Result<Self, VramhubError> {
        todo!("Initialize INTELLECT adapter")
    }
}

#[cfg(feature = "intellect")]
#[async_trait]
impl TrainingFrameworkAdapter for IntellectAdapter {
    fn name(&self) -> &str {
        "intellect"
    }

    async fn load_checkpoint(&mut self, _bytes: &[u8]) -> Result<(), VramhubError> {
        todo!()
    }
    async fn train_step(&mut self, _batch: &[i64]) -> Result<Vec<f32>, VramhubError> {
        todo!()
    }
    fn compress_gradient(&self, _raw: &[f32]) -> Result<CompressedGradient, VramhubError> {
        todo!()
    }
    fn decompress_gradient(&self, _compressed: &[u8]) -> Result<Vec<f32>, VramhubError> {
        todo!()
    }
    async fn apply_gradient(&mut self, _gradient: &[f32], _beta: f32) -> Result<(), VramhubError> {
        todo!()
    }
    async fn save_checkpoint(&self) -> Result<Checkpoint, VramhubError> {
        todo!()
    }
    fn get_assigned_batch(&self, _uid: PeerId, _window: WindowId) -> Vec<i64> {
        todo!()
    }
    fn get_random_batch(&self, _window: WindowId) -> Vec<i64> {
        todo!()
    }
    async fn forward_loss(&self, _batch: &[i64]) -> Result<f32, VramhubError> {
        todo!()
    }
}
