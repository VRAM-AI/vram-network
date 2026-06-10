// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

//! Candle ML adapter — supports CUDA (Linux/Windows), Metal (macOS), and CPU.
//!
//! Feature flags control which backend is compiled in:
//!   --features cuda   → NVIDIA GPU via CUDA (requires CUDA toolkit ≥ 11.8)
//!   --features metal  → Apple Silicon Metal GPU
//!   --features candle → CPU-only fallback
//!
//! Device selection at runtime: CUDA > Metal > CPU (first available wins).
//! Override with VRAMHUB_DEVICE env var: "cuda:0", "cuda:1", "metal", "cpu"

use async_trait::async_trait;
use candle_core::DType;
use candle_core::{Device, Module, Result as CandleResult, Tensor};
use candle_nn::{Optimizer, VarBuilder, VarMap, SGD};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::Mutex;
use vramhub_core::{PeerId, VramhubError, WindowId};

use super::{Checkpoint, CompressedGradient, TrainingFrameworkAdapter};

/// Small MLP model for on-device training.
/// 784 -> 128 -> 10 (similar to a simple MNIST classifier, but we use synthetic data)
pub struct CandleAdapter {
    device: Device,
    varmap: VarMap,
    model: MLP,
    optimizer: Arc<Mutex<SGD>>,
    input_dim: usize,
    output_dim: usize,
}

struct MLP {
    fc1: candle_nn::Linear,
    fc2: candle_nn::Linear,
}

impl MLP {
    fn new(
        vb: VarBuilder,
        input_dim: usize,
        hidden_dim: usize,
        output_dim: usize,
    ) -> CandleResult<Self> {
        let fc1 = candle_nn::linear(input_dim, hidden_dim, vb.pp("fc1"))?;
        let fc2 = candle_nn::linear(hidden_dim, output_dim, vb.pp("fc2"))?;
        Ok(Self { fc1, fc2 })
    }
}

impl Module for MLP {
    fn forward(&self, xs: &Tensor) -> CandleResult<Tensor> {
        let xs = self.fc1.forward(xs)?;
        let xs = xs.relu()?;
        self.fc2.forward(&xs)
    }
}

/// Select the best available compute device.
///
/// Priority: VRAMHUB_DEVICE env var > CUDA > Metal > CPU
fn select_device() -> anyhow::Result<Device> {
    // 1. Explicit override via env var
    if let Ok(spec) = std::env::var("VRAMHUB_DEVICE") {
        return parse_device_spec(&spec);
    }

    // 2. CUDA (compiled in with --features cuda)
    #[cfg(feature = "cuda")]
    {
        match Device::new_cuda(0) {
            Ok(d) => {
                tracing::info!("CUDA device 0 selected");
                return Ok(d);
            }
            Err(e) => tracing::warn!("CUDA available but init failed: {e}"),
        }
    }

    // 3. Metal (compiled in with --features metal, macOS only)
    #[cfg(feature = "metal")]
    {
        match Device::new_metal(0) {
            Ok(d) => {
                tracing::info!("Metal device 0 selected");
                return Ok(d);
            }
            Err(e) => tracing::warn!("Metal available but init failed: {e}"),
        }
    }

    // 4. CPU fallback
    tracing::info!("Using CPU device");
    Ok(Device::Cpu)
}

/// Parse a device spec string like "cuda:0", "cuda:1", "metal", "cpu".
fn parse_device_spec(spec: &str) -> anyhow::Result<Device> {
    let spec = spec.trim().to_lowercase();
    if spec == "cpu" {
        return Ok(Device::Cpu);
    }

    #[cfg(feature = "cuda")]
    if let Some(idx) = spec.strip_prefix("cuda:") {
        let n: usize = idx.parse().unwrap_or(0);
        return Device::new_cuda(n)
            .map_err(|e| anyhow::anyhow!("CUDA device {n} init failed: {e}"));
    }
    #[cfg(feature = "cuda")]
    if spec == "cuda" {
        return Device::new_cuda(0).map_err(|e| anyhow::anyhow!("CUDA device 0 init failed: {e}"));
    }

    #[cfg(feature = "metal")]
    if spec == "metal" {
        return Device::new_metal(0).map_err(|e| anyhow::anyhow!("Metal device init failed: {e}"));
    }

    anyhow::bail!("Unknown device spec {spec:?} — use 'cuda', 'cuda:N', 'metal', or 'cpu'")
}

impl CandleAdapter {
    /// Create a new Candle adapter.
    ///
    /// Device selection (first available):
    ///   1. `VRAMHUB_DEVICE` env var — "cuda:0", "cuda:1", "metal", "cpu"
    ///   2. CUDA (if compiled with --features cuda)
    ///   3. Metal (if compiled with --features metal, macOS only)
    ///   4. CPU
    pub fn new() -> anyhow::Result<Self> {
        let device = select_device()?;
        tracing::info!("Candle using device: {:?}", device);

        let input_dim = 784; // 28x28 image
        let hidden_dim = 128; // Small hidden layer
        let output_dim = 10; // 10 classes

        let varmap = VarMap::new();
        let vb = VarBuilder::from_varmap(&varmap, DType::F32, &device);
        let model = MLP::new(vb, input_dim, hidden_dim, output_dim)?;

        // SGD optimizer with learning rate 0.01
        let optimizer = SGD::new(varmap.all_vars(), 0.01)?;

        Ok(Self {
            device,
            varmap,
            model,
            optimizer: Arc::new(Mutex::new(optimizer)),
            input_dim,
            output_dim,
        })
    }

    /// Sorted variable names for deterministic serialization order
    fn sorted_var_names(&self) -> Vec<String> {
        let data = self.varmap.data().lock().unwrap();
        let mut names: Vec<String> = data.keys().cloned().collect();
        names.sort();
        names
    }

    /// Flatten model parameters to f32 vec (sorted by name for determinism)
    fn params_to_vec(&self) -> CandleResult<Vec<f32>> {
        let names = self.sorted_var_names();
        let data = self.varmap.data().lock().unwrap();
        let mut params = Vec::new();
        for name in &names {
            if let Some(var) = data.get(name) {
                let flat = var.as_tensor().flatten_all()?;
                let vals: Vec<f32> = flat.to_vec1()?;
                params.extend(vals);
            }
        }
        Ok(params)
    }

    /// Load parameters from f32 vec (sorted by name for determinism)
    fn vec_to_params(&mut self, data: &[f32]) -> CandleResult<()> {
        let names = self.sorted_var_names();
        let var_data = self.varmap.data().lock().unwrap();
        let mut offset = 0;
        for name in &names {
            if let Some(var) = var_data.get(name) {
                let tensor = var.as_tensor();
                let shape = tensor.shape().clone();
                let numel = tensor.elem_count();
                if offset >= data.len() {
                    break;
                }
                let end = (offset + numel).min(data.len());
                let chunk = &data[offset..end];
                if chunk.len() == numel {
                    let new_tensor = Tensor::from_vec(chunk.to_vec(), shape, &self.device)?;
                    var.set(&new_tensor)?;
                }
                offset += numel;
            }
        }
        Ok(())
    }

    /// Generate synthetic data batch
    fn generate_batch(&self, seed: u64, batch_size: usize) -> (Tensor, Tensor) {
        let mut rng = seed;

        // Generate synthetic inputs
        let mut inputs = Vec::with_capacity(batch_size * self.input_dim);
        let mut labels = Vec::with_capacity(batch_size);

        for _b in 0..batch_size {
            // LCG for reproducibility
            rng = rng
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let label = (rng % self.output_dim as u64) as usize;
            labels.push(label as f32);

            for i in 0..self.input_dim {
                rng = rng
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let signal = if i % self.output_dim == label {
                    0.5f32
                } else {
                    0.0f32
                };
                let noise = (rng as f32 / u64::MAX as f32) * 0.1f32;
                inputs.push(signal + noise);
            }
        }

        let inputs = Tensor::from_vec(inputs, (batch_size, self.input_dim), &self.device).unwrap();
        let labels = Tensor::from_vec(labels, batch_size, &self.device)
            .unwrap()
            .to_dtype(candle_core::DType::U32)
            .unwrap();

        (inputs, labels)
    }
}

#[async_trait]
impl TrainingFrameworkAdapter for CandleAdapter {
    fn name(&self) -> &str {
        "candle-mlp-metal"
    }

    async fn load_checkpoint(&mut self, checkpoint_bytes: &[u8]) -> Result<(), VramhubError> {
        if checkpoint_bytes.is_empty() {
            // Initialize with random weights (default)
            return Ok(());
        }

        let params = bytes_to_f32(checkpoint_bytes);
        self.vec_to_params(&params)
            .map_err(|e| VramhubError::Internal(format!("Failed to load checkpoint: {e}")))?;
        Ok(())
    }

    async fn train_step(&mut self, batch: &[i64]) -> Result<Vec<f32>, VramhubError> {
        // Use batch as seed
        let seed = batch.first().copied().unwrap_or(0) as u64;
        let batch_size = 16;

        let (inputs, labels) = self.generate_batch(seed, batch_size);

        // Forward pass
        let logits = self
            .model
            .forward(&inputs)
            .map_err(|e| VramhubError::Internal(format!("Forward failed: {e}")))?;

        // Compute loss
        let loss = cross_entropy_loss(&logits, &labels)
            .map_err(|e| VramhubError::Internal(format!("Loss computation failed: {e}")))?;

        // Backward pass - returns GradStore
        let grads = loss
            .backward()
            .map_err(|e| VramhubError::Internal(format!("Backward failed: {e}")))?;

        // Get parameter gradients (sorted by name for determinism)
        // Collect in a block so MutexGuard is dropped before the async optimizer.await
        let grad_vec = {
            let names = self.sorted_var_names();
            let var_data = self.varmap.data().lock().unwrap();
            let mut grad_vec = Vec::new();
            for name in &names {
                if let Some(var) = var_data.get(name) {
                    let grad = grads.get(var.as_tensor()).ok_or_else(|| {
                        VramhubError::Internal(format!("Missing gradient for {name}"))
                    })?;
                    let flat = grad
                        .flatten_all()
                        .map_err(|e| VramhubError::Internal(format!("Flatten failed: {e}")))?;
                    let data: Vec<f32> = flat
                        .to_vec1()
                        .map_err(|e| VramhubError::Internal(format!("To vec failed: {e}")))?;
                    grad_vec.extend(data);
                }
            }
            grad_vec
        }; // MutexGuard dropped here

        // Optimizer step - use backward_step with loss tensor
        let mut opt = self.optimizer.lock().await;
        opt.backward_step(&loss)
            .map_err(|e| VramhubError::Internal(format!("Optimizer step failed: {e}")))?;
        drop(opt);

        Ok(grad_vec)
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
        let names = self.sorted_var_names();
        let var_data = self.varmap.data().lock().unwrap();
        let mut offset = 0;
        for name in &names {
            if let Some(var) = var_data.get(name) {
                let tensor = var.as_tensor();
                let shape = tensor.shape().clone();
                let numel = tensor.elem_count();
                if offset >= gradient.len() {
                    break;
                }
                let end = (offset + numel).min(gradient.len());
                let chunk = &gradient[offset..end];
                if chunk.len() == numel {
                    let scaled: Vec<f32> = chunk.iter().map(|&g| g * beta).collect();
                    let scaled_tensor =
                        Tensor::from_vec(scaled, shape, &self.device).map_err(|e| {
                            VramhubError::Internal(format!("Tensor creation failed: {e}"))
                        })?;
                    let new_val = tensor.sub(&scaled_tensor).map_err(|e| {
                        VramhubError::Internal(format!("Gradient apply failed: {e}"))
                    })?;
                    var.set(&new_val)
                        .map_err(|e| VramhubError::Internal(format!("Var set failed: {e}")))?;
                }
                offset += numel;
            }
        }
        Ok(())
    }

    async fn save_checkpoint(&self) -> Result<Checkpoint, VramhubError> {
        let params = self
            .params_to_vec()
            .map_err(|e| VramhubError::Internal(format!("Failed to save checkpoint: {e}")))?;
        let data = f32_to_bytes(&params);
        let hash: [u8; 32] = Sha256::digest(&data).into();
        Ok(Checkpoint { data, hash })
    }

    fn get_assigned_batch(&self, uid: PeerId, window: WindowId) -> Vec<i64> {
        let seed = uid ^ window;
        (0..16)
            .map(|i| {
                let state = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407)
                    .wrapping_add(i);
                state as i64
            })
            .collect()
    }

    fn get_random_batch(&self, window: WindowId) -> Vec<i64> {
        let seed = window;
        (0..16)
            .map(|i| {
                let state = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407)
                    .wrapping_add(i);
                state as i64
            })
            .collect()
    }

    async fn forward_loss(&self, batch: &[i64]) -> Result<f32, VramhubError> {
        let seed = batch.first().copied().unwrap_or(0) as u64;
        let batch_size = 16;

        let (inputs, labels) = self.generate_batch(seed, batch_size);

        let logits = self
            .model
            .forward(&inputs)
            .map_err(|e| VramhubError::Internal(format!("Forward failed: {e}")))?;

        let loss = cross_entropy_loss(&logits, &labels)
            .map_err(|e| VramhubError::Internal(format!("Loss computation failed: {e}")))?;

        let loss_val = loss
            .to_scalar::<f32>()
            .map_err(|e| VramhubError::Internal(format!("To scalar failed: {e}")))?;

        Ok(loss_val)
    }
}

/// Simple cross entropy loss
fn cross_entropy_loss(logits: &Tensor, labels: &Tensor) -> CandleResult<Tensor> {
    // logits: [batch_size, num_classes], labels: [batch_size] with class indices
    let log_softmax = candle_nn::ops::log_softmax(logits, 1)?;
    // gather expects indices, labels should be [batch_size, 1] for gather on dim 1
    let labels_expanded = labels.unsqueeze(1)?; // [batch_size] -> [batch_size, 1]
    let gathered = log_softmax.gather(&labels_expanded, 1)?; // [batch_size, 1]
    let loss = gathered.neg()?.squeeze(1)?.mean_all()?; // [batch_size] -> scalar
    Ok(loss)
}

fn f32_to_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for &f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

pub fn bytes_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candle_adapter_creates() {
        let adapter = CandleAdapter::new();
        assert!(adapter.is_ok());
    }

    #[tokio::test]
    async fn candle_training_reduces_loss() {
        let mut adapter = CandleAdapter::new().unwrap();
        let batch = adapter.get_assigned_batch(0, 1);

        let loss_before = adapter.forward_loss(&batch).await.unwrap();

        // Train for 10 steps
        for _ in 0..10 {
            adapter.train_step(&batch).await.unwrap();
        }

        let loss_after = adapter.forward_loss(&batch).await.unwrap();

        // Loss should decrease
        assert!(
            loss_after < loss_before,
            "Loss should decrease: before={loss_before}, after={loss_after}"
        );
    }
}
