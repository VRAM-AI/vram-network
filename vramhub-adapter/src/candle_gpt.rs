// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

//! Trainable nano-GPT adapter for VRAM network.
//!
//! Architecture: 6-layer transformer, 384-dim, 6-head, 1024 context — ~10M params.
//! Tokenizer:    tiktoken cl100k_base (100k vocab, same as GPT-4).
//! Dataset:      FineWeb-edu pre-tokenized shards, deterministically assigned
//!               by (uid, window) seed.
//! GPU support:  CUDA (--features cuda) > Metal (--features metal) > CPU.
//!
//! Run:
//!   cargo run --bin vramhub-miner --features cuda    # NVIDIA
//!   cargo run --bin vramhub-miner --features metal   # Apple Silicon
//!   cargo run --bin vramhub-miner --features candle  # CPU

use async_trait::async_trait;
use candle_core::{DType, Device, Module, Tensor, D};
use candle_nn::{
    embedding, layer_norm, linear, linear_no_bias, AdamW, Embedding, LayerNorm, Linear, Optimizer,
    ParamsAdamW, VarBuilder, VarMap,
};
use sha2::{Digest as Sha2Digest, Sha256};
use std::sync::Arc;
use tokio::sync::Mutex;
use vramhub_core::{PeerId, VramhubError, WindowId};

use super::{Checkpoint, CompressedGradient, TrainingFrameworkAdapter};

// ── Hyperparameters ────────────────────────────────────────────────────────────

const VOCAB_SIZE: usize = 100_277; // cl100k_base
const N_LAYER: usize = 6;
const N_HEAD: usize = 6;
const N_EMBD: usize = 384;
const BLOCK_SIZE: usize = 256; // context window
const BATCH_SIZE: usize = 4; // sequences per window
const LEARNING_RATE: f64 = 3e-4;

// ── Device selection ──────────────────────────────────────────────────────────

pub fn select_device() -> anyhow::Result<Device> {
    if let Ok(spec) = std::env::var("VRAMHUB_DEVICE") {
        return parse_device_spec(&spec);
    }
    #[cfg(feature = "cuda")]
    match Device::new_cuda(0) {
        Ok(d) => {
            tracing::info!("Using CUDA device 0");
            return Ok(d);
        }
        Err(e) => tracing::warn!("CUDA init failed: {e}"),
    }
    #[cfg(feature = "metal")]
    match Device::new_metal(0) {
        Ok(d) => {
            tracing::info!("Using Metal device 0");
            return Ok(d);
        }
        Err(e) => tracing::warn!("Metal init failed: {e}"),
    }
    tracing::info!("Using CPU");
    Ok(Device::Cpu)
}

fn parse_device_spec(spec: &str) -> anyhow::Result<Device> {
    match spec.trim().to_lowercase().as_str() {
        "cpu" => Ok(Device::Cpu),
        #[cfg(feature = "cuda")]
        s if s.starts_with("cuda") => {
            let n = s
                .strip_prefix("cuda:")
                .and_then(|x| x.parse().ok())
                .unwrap_or(0);
            Device::new_cuda(n).map_err(|e| anyhow::anyhow!("CUDA:{n} failed: {e}"))
        }
        #[cfg(feature = "metal")]
        "metal" => Device::new_metal(0).map_err(|e| anyhow::anyhow!("Metal failed: {e}")),
        s => anyhow::bail!("Unknown device {s:?}"),
    }
}

// ── Model ─────────────────────────────────────────────────────────────────────

struct CausalSelfAttention {
    c_attn: Linear, // q, k, v projection (3 * n_embd)
    c_proj: Linear, // output projection
    n_head: usize,
    n_embd: usize,
}

impl CausalSelfAttention {
    fn new(vb: VarBuilder, n_head: usize, n_embd: usize) -> candle_core::Result<Self> {
        let c_attn = linear(n_embd, 3 * n_embd, vb.pp("c_attn"))?;
        let c_proj = linear(n_embd, n_embd, vb.pp("c_proj"))?;
        Ok(Self {
            c_attn,
            c_proj,
            n_head,
            n_embd,
        })
    }

    fn forward(&self, x: &Tensor) -> candle_core::Result<Tensor> {
        let (b, t, c) = x.dims3()?;
        let head_size = c / self.n_head;

        let qkv = self.c_attn.forward(x)?; // (B, T, 3*C)
                                           // .contiguous() required after transpose — candle CPU matmul needs contiguous tensors
        let q = qkv
            .narrow(D::Minus1, 0, self.n_embd)?
            .reshape((b, t, self.n_head, head_size))?
            .transpose(1, 2)?
            .contiguous()?;
        let k = qkv
            .narrow(D::Minus1, self.n_embd, self.n_embd)?
            .reshape((b, t, self.n_head, head_size))?
            .transpose(1, 2)?
            .contiguous()?;
        let v = qkv
            .narrow(D::Minus1, 2 * self.n_embd, self.n_embd)?
            .reshape((b, t, self.n_head, head_size))?
            .transpose(1, 2)?
            .contiguous()?;

        // Scaled dot-product attention with causal mask
        let scale = (head_size as f64).sqrt();
        let att = (q.matmul(&k.transpose(D::Minus2, D::Minus1)?.contiguous()?)? / scale)?;

        // Causal mask: upper triangle = -inf, lower = 0
        let causal: Vec<f32> = (0..t * t)
            .map(|k| {
                if (k % t) <= (k / t) {
                    0.0f32
                } else {
                    f32::NEG_INFINITY
                }
            })
            .collect();
        let mask = Tensor::from_vec(causal, (1, 1, t, t), x.device())?;
        let att = (att + mask.broadcast_as((b, self.n_head, t, t))?)?;
        let att = candle_nn::ops::softmax(&att, D::Minus1)?;

        let y = att
            .matmul(&v)? // (B, nh, T, hs)
            .transpose(1, 2)?
            .contiguous()? // (B, T, nh, hs)
            .reshape((b, t, c))?; // (B, T, C)
        self.c_proj.forward(&y)
    }
}

struct MLP {
    fc: Linear,
    proj: Linear,
}

impl MLP {
    fn new(vb: VarBuilder, n_embd: usize) -> candle_core::Result<Self> {
        Ok(Self {
            fc: linear(n_embd, 4 * n_embd, vb.pp("fc"))?,
            proj: linear(4 * n_embd, n_embd, vb.pp("proj"))?,
        })
    }
    fn forward(&self, x: &Tensor) -> candle_core::Result<Tensor> {
        let x = self.fc.forward(x)?.gelu_erf()?;
        self.proj.forward(&x)
    }
}

struct Block {
    ln1: LayerNorm,
    attn: CausalSelfAttention,
    ln2: LayerNorm,
    mlp: MLP,
}

impl Block {
    fn new(vb: VarBuilder, n_head: usize, n_embd: usize) -> candle_core::Result<Self> {
        Ok(Self {
            ln1: layer_norm(n_embd, 1e-5, vb.pp("ln1"))?,
            attn: CausalSelfAttention::new(vb.pp("attn"), n_head, n_embd)?,
            ln2: layer_norm(n_embd, 1e-5, vb.pp("ln2"))?,
            mlp: MLP::new(vb.pp("mlp"), n_embd)?,
        })
    }
    fn forward(&self, x: &Tensor) -> candle_core::Result<Tensor> {
        let x = (x + self.attn.forward(&self.ln1.forward(x)?)?)?;
        let x = (&x + self.mlp.forward(&self.ln2.forward(&x)?)?)?;
        Ok(x)
    }
}

struct NanoGPT {
    wte: Embedding, // token embedding
    wpe: Embedding, // position embedding
    blocks: Vec<Block>,
    ln_f: LayerNorm,
    lm_head: Linear,
    device: Device,
}

impl NanoGPT {
    fn new(vb: VarBuilder, device: &Device) -> candle_core::Result<Self> {
        let wte = embedding(VOCAB_SIZE, N_EMBD, vb.pp("wte"))?;
        let wpe = embedding(BLOCK_SIZE, N_EMBD, vb.pp("wpe"))?;
        let blocks = (0..N_LAYER)
            .map(|i| Block::new(vb.pp(format!("block.{i}")), N_HEAD, N_EMBD))
            .collect::<candle_core::Result<Vec<_>>>()?;
        let ln_f = layer_norm(N_EMBD, 1e-5, vb.pp("ln_f"))?;
        let lm_head = linear_no_bias(N_EMBD, VOCAB_SIZE, vb.pp("lm_head"))?;
        Ok(Self {
            wte,
            wpe,
            blocks,
            ln_f,
            lm_head,
            device: device.clone(),
        })
    }

    /// Forward pass — returns logits (B, T, V).
    fn forward(&self, idx: &Tensor) -> candle_core::Result<Tensor> {
        let (b, t) = idx.dims2()?;
        let tok_emb = self.wte.forward(idx)?;
        let pos = Tensor::arange(0u32, t as u32, &self.device)?;
        let pos_emb = self.wpe.forward(&pos)?;
        let mut x = (tok_emb + pos_emb.broadcast_as((b, t, N_EMBD))?)?;
        for block in &self.blocks {
            x = block.forward(&x)?;
        }
        let x = self.ln_f.forward(&x)?;
        self.lm_head.forward(&x)
    }

    /// Cross-entropy loss over next-token prediction.
    fn loss(&self, idx: &Tensor) -> candle_core::Result<Tensor> {
        let (b, t) = idx.dims2()?;
        // Forward only on the context window (first t-1 tokens), so position
        // indices stay in [0, BLOCK_SIZE-1] — wpe has exactly BLOCK_SIZE rows.
        let context = idx.narrow(1, 0, t - 1)?; // (B, T-1)
        let logits = self.forward(&context)?; // (B, T-1, V)
        let logits = logits.reshape((b * (t - 1), VOCAB_SIZE))?;
        let targets = idx.narrow(1, 1, t - 1)?.reshape((b * (t - 1),))?;
        candle_nn::loss::cross_entropy(&logits, &targets)
    }
}

// ── Dataset: FineWeb-edu shards ───────────────────────────────────────────────

/// Download a pre-tokenized FineWeb-edu shard for a given seed.
/// Shards are ~100MB Arrow/Parquet files hosted on HuggingFace.
/// We cache them at ~/.vramhub/fineweb/<shard_id>.bin
async fn get_token_shard(seed: u64) -> anyhow::Result<Vec<u32>> {
    // FineWeb-edu has 1000 shards; deterministically assign by seed
    let shard_idx = seed % 100; // first 100 shards for testnet
    let cache_dir = dirs_cache_dir().join(format!("fineweb_shard_{shard_idx:03}.bin"));

    if cache_dir.exists() {
        let bytes = tokio::fs::read(&cache_dir).await?;
        return Ok(bytes_to_u32(&bytes));
    }

    // Download from HuggingFace datasets (pre-tokenized cl100k_base, u32 per token)
    let url = format!(
        "https://huggingface.co/datasets/HuggingFaceFW/fineweb-edu/resolve/main/sample/10BT/\
         fineweb-edu-tokenized-{shard_idx:06}.bin"
    );
    tracing::info!(shard_idx, "Downloading FineWeb-edu shard (first time only)");
    let client = reqwest::Client::new();
    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let bytes = resp.bytes().await?.to_vec();
            let _ = tokio::fs::create_dir_all(cache_dir.parent().unwrap()).await;
            let _ = tokio::fs::write(&cache_dir, &bytes).await;
            Ok(bytes_to_u32(&bytes))
        }
        _ => {
            tracing::warn!("FineWeb shard unavailable, using synthetic tokens");
            Ok(synthetic_tokens(seed, 50_000))
        }
    }
}

fn dirs_cache_dir() -> std::path::PathBuf {
    let base = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    std::path::PathBuf::from(base)
        .join(".vramhub")
        .join("fineweb")
}

fn bytes_to_u32(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

/// Fallback: lcg synthetic tokens (deterministic, exercises training loop)
fn synthetic_tokens(seed: u64, n: usize) -> Vec<u32> {
    let mut s = seed;
    (0..n)
        .map(|_| {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (s % VOCAB_SIZE as u64) as u32
        })
        .collect()
}

/// Extract a (BATCH_SIZE, BLOCK_SIZE+1) batch from a shard at a given offset.
fn extract_batch(tokens: &[u32], uid: u64, window: u64) -> Vec<Vec<u32>> {
    if tokens.len() < BLOCK_SIZE + 1 {
        return vec![];
    }
    let mut batches = Vec::with_capacity(BATCH_SIZE);
    for b in 0..BATCH_SIZE {
        let seed = uid
            .wrapping_mul(1000003)
            .wrapping_add(window)
            .wrapping_add(b as u64);
        let offset = (seed as usize) % (tokens.len() - BLOCK_SIZE - 1);
        batches.push(tokens[offset..offset + BLOCK_SIZE + 1].to_vec());
    }
    batches
}

// ── Checkpoint: raw f32 serialization ────────────────────────────────────────

fn params_to_bytes(varmap: &VarMap) -> candle_core::Result<Vec<u8>> {
    let data = varmap.data().lock().unwrap();
    let mut names: Vec<&String> = data.keys().collect();
    names.sort();
    let mut out = Vec::new();
    for name in names {
        let vals: Vec<f32> = data[name].as_tensor().flatten_all()?.to_vec1()?;
        for v in vals {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    Ok(out)
}

fn bytes_to_f32_vec(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

// ── Adapter ───────────────────────────────────────────────────────────────────

pub struct CandleGptAdapter {
    device: Device,
    varmap: VarMap,
    model: NanoGPT,
    optimizer: Arc<Mutex<AdamW>>,
    /// Cached token shard for current window
    token_cache: Option<(u64, Vec<u32>)>, // (window, tokens)
}

impl CandleGptAdapter {
    pub fn new() -> anyhow::Result<Self> {
        let device = select_device()?;
        let varmap = VarMap::new();
        let vb = VarBuilder::from_varmap(&varmap, DType::F32, &device);
        let model = NanoGPT::new(vb, &device).map_err(|e| anyhow::anyhow!("Model init: {e}"))?;
        let optimizer = AdamW::new(
            varmap.all_vars(),
            ParamsAdamW {
                lr: LEARNING_RATE,
                ..Default::default()
            },
        )
        .map_err(|e| anyhow::anyhow!("Optimizer: {e}"))?;
        Ok(Self {
            device,
            varmap,
            model,
            optimizer: Arc::new(Mutex::new(optimizer)),
            token_cache: None,
        })
    }

    fn n_params(&self) -> usize {
        let data = self.varmap.data().lock().unwrap();
        data.values().map(|v| v.as_tensor().elem_count()).sum()
    }
}

#[async_trait]
impl TrainingFrameworkAdapter for CandleGptAdapter {
    fn name(&self) -> &str {
        "nano-gpt-10m"
    }

    async fn load_checkpoint(&mut self, checkpoint_bytes: &[u8]) -> Result<(), VramhubError> {
        if checkpoint_bytes.is_empty() {
            tracing::info!(
                params = self.n_params(),
                "Initialized fresh nano-GPT (~10M params)"
            );
            return Ok(());
        }
        let floats = bytes_to_f32_vec(checkpoint_bytes);
        let data = self.varmap.data().lock().unwrap();
        let mut names: Vec<&String> = data.keys().collect();
        names.sort();
        let mut offset = 0usize;
        for name in names {
            let var = &data[name];
            let n = var.as_tensor().elem_count();
            if offset + n > floats.len() {
                break;
            }
            let t = Tensor::from_slice(
                &floats[offset..offset + n],
                var.as_tensor().shape(),
                &self.device,
            )
            .map_err(|e| VramhubError::Internal(e.to_string()))?;
            var.set(&t)
                .map_err(|e| VramhubError::Internal(e.to_string()))?;
            offset += n;
        }
        tracing::info!("Loaded checkpoint ({} bytes)", checkpoint_bytes.len());
        Ok(())
    }

    async fn train_step(&mut self, batch: &[i64]) -> Result<Vec<f32>, VramhubError> {
        // batch[0] = uid, batch[1] = window (packed by miner loop)
        let uid = batch.first().copied().unwrap_or(0) as u64;
        let window = batch.get(1).copied().unwrap_or(0) as u64;

        // Load or reuse token shard
        let tokens = if let Some((w, ref t)) = self.token_cache {
            if w == window {
                t.clone()
            } else {
                let t = get_token_shard(uid ^ window)
                    .await
                    .map_err(|e| VramhubError::Internal(e.to_string()))?;
                self.token_cache = Some((window, t.clone()));
                t
            }
        } else {
            let t = get_token_shard(uid ^ window)
                .await
                .map_err(|e| VramhubError::Internal(e.to_string()))?;
            self.token_cache = Some((window, t.clone()));
            t
        };

        let batches = extract_batch(&tokens, uid, window);
        if batches.is_empty() {
            return Err(VramhubError::Internal("Empty token batch".into()));
        }

        // Build (BATCH, BLOCK_SIZE+1) u32 tensor
        let flat: Vec<u32> = batches.iter().flatten().copied().collect();
        let idx = Tensor::from_vec(flat, (BATCH_SIZE, BLOCK_SIZE + 1), &self.device)
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        // Forward + loss
        let loss = self
            .model
            .loss(&idx)
            .map_err(|e| VramhubError::Internal(format!("Loss: {e}")))?;
        let loss_val = loss
            .to_scalar::<f32>()
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        // Backward + optimizer step — collect gradients before step
        let grads = loss
            .backward()
            .map_err(|e| VramhubError::Internal(format!("Backward: {e}")))?;

        let grad_vec = {
            let data = self.varmap.data().lock().unwrap();
            let mut names: Vec<&String> = data.keys().collect();
            names.sort();
            let mut out = Vec::new();
            for name in &names {
                if let Some(g) = grads.get(data[*name].as_tensor()) {
                    let v: Vec<f32> = g
                        .flatten_all()
                        .map_err(|e| VramhubError::Internal(e.to_string()))?
                        .to_vec1()
                        .map_err(|e| VramhubError::Internal(e.to_string()))?;
                    out.extend(v);
                }
            }
            out
        };

        let mut opt = self.optimizer.lock().await;
        opt.backward_step(&loss)
            .map_err(|e| VramhubError::Internal(format!("Optimizer: {e}")))?;
        drop(opt);

        tracing::info!(
            window,
            uid,
            loss = loss_val,
            grad_params = grad_vec.len(),
            "Train step"
        );
        Ok(grad_vec)
    }

    fn compress_gradient(&self, raw: &[f32]) -> Result<CompressedGradient, VramhubError> {
        let data = crate::training::compress_topk_f16(raw, raw.len() / 10); // top-10%
        let hash = hex::encode(Sha256::digest(&data));
        let size = data.len() as u64;
        Ok(CompressedGradient {
            data,
            content_hash: hash,
            size_bytes: size,
        })
    }

    fn decompress_gradient(&self, compressed: &[u8]) -> Result<Vec<f32>, VramhubError> {
        crate::training::decompress_topk_f16(compressed)
    }

    async fn apply_gradient(&mut self, gradient: &[f32], beta: f32) -> Result<(), VramhubError> {
        let data = self.varmap.data().lock().unwrap();
        let mut names: Vec<&String> = data.keys().collect();
        names.sort();
        let mut offset = 0;
        for name in &names {
            let var = &data[*name];
            let n = var.as_tensor().elem_count();
            if offset + n > gradient.len() {
                break;
            }
            let chunk = &gradient[offset..offset + n];
            let g = Tensor::from_slice(chunk, var.as_tensor().shape(), &self.device)
                .map_err(|e| VramhubError::Internal(e.to_string()))?;
            let scaled = (g * beta as f64).map_err(|e| VramhubError::Internal(e.to_string()))?;
            let updated = var
                .as_tensor()
                .sub(&scaled)
                .map_err(|e| VramhubError::Internal(e.to_string()))?;
            var.set(&updated)
                .map_err(|e| VramhubError::Internal(e.to_string()))?;
            offset += n;
        }
        Ok(())
    }

    async fn save_checkpoint(&self) -> Result<Checkpoint, VramhubError> {
        let data =
            params_to_bytes(&self.varmap).map_err(|e| VramhubError::Internal(e.to_string()))?;
        let hash: [u8; 32] = Sha256::digest(&data).into();
        Ok(Checkpoint { data, hash })
    }

    fn get_assigned_batch(&self, uid: PeerId, window: WindowId) -> Vec<i64> {
        // Pack uid and window into the batch slice — train_step unpacks them
        vec![uid as i64, window as i64]
    }

    fn get_random_batch(&self, window: WindowId) -> Vec<i64> {
        vec![0, window as i64]
    }

    async fn forward_loss(&self, batch: &[i64]) -> Result<f32, VramhubError> {
        let uid = batch.first().copied().unwrap_or(0) as u64;
        let window = batch.get(1).copied().unwrap_or(0) as u64;
        let tokens = get_token_shard(uid ^ window)
            .await
            .map_err(|e| VramhubError::Internal(e.to_string()))?;
        let batches = extract_batch(&tokens, uid, window);
        if batches.is_empty() {
            return Ok(f32::MAX);
        }
        let flat: Vec<u32> = batches.iter().flatten().copied().collect();
        let idx = Tensor::from_vec(flat, (BATCH_SIZE, BLOCK_SIZE + 1), &self.device)
            .map_err(|e| VramhubError::Internal(e.to_string()))?;
        let loss = self
            .model
            .loss(&idx)
            .map_err(|e| VramhubError::Internal(e.to_string()))?
            .to_scalar::<f32>()
            .map_err(|e| VramhubError::Internal(e.to_string()))?;
        Ok(loss)
    }
}
