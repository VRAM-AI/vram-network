// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

//! Python sidecar adapter.
//!
//! Lets any PyTorch (or JAX/TF) training script act as a VRAM miner.
//! The Rust miner handles chain, R2, compression, and rewards.
//! The sidecar handles the actual forward/backward pass.
//!
//! # Security notes
//!
//! - The sidecar server must bind to `127.0.0.1` (loopback), not `0.0.0.0`.
//!   Binding to all interfaces exposes raw gradient data and checkpoint weights
//!   to any process on the network.
//! - The sidecar has no authentication by default. If you run the miner and
//!   sidecar on separate machines, add a shared secret or mTLS between them.
//! - Gradients returned by the sidecar are uploaded to R2 and evaluated by
//!   validators. A malicious sidecar could submit zero gradients or noise,
//!   resulting in low OpenSkill scores and reduced rewards — the TEE enforces
//!   honest gradient quality, not honest gradient origin.
//! - The 60-second health-check retry loop at startup will keep retrying until
//!   the sidecar responds. Make sure the sidecar URL is correct before starting
//!   the miner to avoid waiting on a misconfigured endpoint.
//!
//! ## Protocol
//!
//! The sidecar exposes a local HTTP server (default: http://127.0.0.1:7070):
//!
//!   POST /train
//!     Body:  { "uid": u64, "window": u64 }
//!     Reply: { "gradient": [f32, ...], "loss": f32 }
//!
//!   POST /load_checkpoint
//!     Body:  { "data": "base64..." }
//!     Reply: { "ok": true }
//!
//!   POST /save_checkpoint
//!     Body:  {}
//!     Reply: { "data": "base64..." }
//!
//!   POST /forward_loss
//!     Body:  { "uid": u64, "window": u64 }
//!     Reply: { "loss": f32 }
//!
//!   GET /health
//!     Reply: { "status": "ok", "model": "gpt2-124m", "device": "cuda:0" }
//!
//! ## Quick start
//!
//!   # Terminal 1: start the Python sidecar
//!   pip install -r scripts/requirements.txt
//!   python scripts/vram_trainer.py --port 7070
//!
//!   # Terminal 2: start the miner
//!   VRAMHUB_SIDECAR_URL=http://127.0.0.1:7070 cargo run --bin vramhub-miner --features sidecar

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use vramhub_core::{PeerId, VramhubError, WindowId};

use super::{Checkpoint, CompressedGradient, TrainingFrameworkAdapter};

// ── Config ────────────────────────────────────────────────────────────────────

fn sidecar_url() -> String {
    std::env::var("VRAMHUB_SIDECAR_URL").unwrap_or_else(|_| "http://127.0.0.1:7070".to_string())
}

// ── Wire types ────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct TrainRequest {
    uid: u64,
    window: u64,
}

#[derive(Deserialize)]
struct TrainResponse {
    gradient: Vec<f32>,
    loss: f32,
}

#[derive(Serialize)]
struct LoadCheckpointRequest {
    data: String,
} // base64

#[derive(Deserialize)]
struct SaveCheckpointResponse {
    data: String,
} // base64

#[derive(Serialize)]
struct ForwardLossRequest {
    uid: u64,
    window: u64,
}

#[derive(Deserialize)]
struct ForwardLossResponse {
    loss: f32,
}

#[derive(Deserialize)]
struct HealthResponse {
    status: String,
    model: String,
    device: String,
}

// ── Adapter ───────────────────────────────────────────────────────────────────

pub struct SidecarAdapter {
    client: reqwest::Client,
    url: String,
    model_name: String,
}

impl SidecarAdapter {
    /// Create and verify the sidecar is reachable.
    pub async fn new() -> Result<Self, VramhubError> {
        let url = sidecar_url();
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(600)) // 10 min for long train steps
            .build()
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        // Health check with retry
        let model_name = Self::wait_for_sidecar(&client, &url).await?;
        Ok(Self {
            client,
            url,
            model_name,
        })
    }

    async fn wait_for_sidecar(client: &reqwest::Client, url: &str) -> Result<String, VramhubError> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            match client.get(format!("{url}/health")).send().await {
                Ok(r) if r.status().is_success() => {
                    let h: HealthResponse = r
                        .json()
                        .await
                        .map_err(|e| VramhubError::Internal(e.to_string()))?;
                    tracing::info!(model = %h.model, device = %h.device, "Sidecar ready");
                    return Ok(h.model);
                }
                _ => {
                    if std::time::Instant::now() >= deadline {
                        return Err(VramhubError::Internal(format!(
                            "Sidecar at {url} did not respond within 60s. \
                             Start it with: python scripts/vram_trainer.py"
                        )));
                    }
                    tracing::info!("Waiting for sidecar at {url}…");
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                }
            }
        }
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }
}

#[async_trait]
impl TrainingFrameworkAdapter for SidecarAdapter {
    fn name(&self) -> &str {
        self.model_name()
    }

    async fn load_checkpoint(&mut self, checkpoint_bytes: &[u8]) -> Result<(), VramhubError> {
        let data = base64_encode(checkpoint_bytes);
        self.client
            .post(format!("{}/load_checkpoint", self.url))
            .json(&LoadCheckpointRequest { data })
            .send()
            .await
            .map_err(|e| VramhubError::Internal(format!("Sidecar load_checkpoint: {e}")))?;
        Ok(())
    }

    async fn train_step(&mut self, batch: &[i64]) -> Result<Vec<f32>, VramhubError> {
        let uid = batch.first().copied().unwrap_or(0) as u64;
        let window = batch.get(1).copied().unwrap_or(0) as u64;

        let resp: TrainResponse = self
            .client
            .post(format!("{}/train", self.url))
            .json(&TrainRequest { uid, window })
            .send()
            .await
            .map_err(|e| VramhubError::Internal(format!("Sidecar /train: {e}")))?
            .json()
            .await
            .map_err(|e| VramhubError::Internal(format!("Sidecar /train parse: {e}")))?;

        tracing::info!(
            uid,
            window,
            loss = resp.loss,
            params = resp.gradient.len(),
            "Sidecar train"
        );
        Ok(resp.gradient)
    }

    fn compress_gradient(&self, raw: &[f32]) -> Result<CompressedGradient, VramhubError> {
        let data = crate::training::compress_topk_f16(raw, raw.len() / 10);
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

    async fn apply_gradient(&mut self, _gradient: &[f32], _beta: f32) -> Result<(), VramhubError> {
        // Aggregated gradients are applied by the sidecar's own optimizer each window.
        // For now this is a no-op; a future version can POST the aggregated grad.
        Ok(())
    }

    async fn save_checkpoint(&self) -> Result<Checkpoint, VramhubError> {
        let resp: SaveCheckpointResponse = self
            .client
            .post(format!("{}/save_checkpoint", self.url))
            .json(&serde_json::json!({}))
            .send()
            .await
            .map_err(|e| VramhubError::Internal(format!("Sidecar /save_checkpoint: {e}")))?
            .json()
            .await
            .map_err(|e| VramhubError::Internal(format!("Sidecar /save_checkpoint parse: {e}")))?;

        let data = base64_decode(&resp.data)
            .map_err(|e| VramhubError::Internal(format!("base64 decode: {e}")))?;
        let hash: [u8; 32] = Sha256::digest(&data).into();
        Ok(Checkpoint { data, hash })
    }

    fn get_assigned_batch(&self, uid: PeerId, window: WindowId) -> Vec<i64> {
        vec![uid as i64, window as i64]
    }

    fn get_random_batch(&self, window: WindowId) -> Vec<i64> {
        vec![0, window as i64]
    }

    async fn forward_loss(&self, batch: &[i64]) -> Result<f32, VramhubError> {
        let uid = batch.first().copied().unwrap_or(0) as u64;
        let window = batch.get(1).copied().unwrap_or(0) as u64;

        let resp: ForwardLossResponse = self
            .client
            .post(format!("{}/forward_loss", self.url))
            .json(&ForwardLossRequest { uid, window })
            .send()
            .await
            .map_err(|e| VramhubError::Internal(format!("Sidecar /forward_loss: {e}")))?
            .json()
            .await
            .map_err(|e| VramhubError::Internal(format!("Sidecar /forward_loss parse: {e}")))?;

        Ok(resp.loss)
    }
}

// ── base64 helpers (no extra dep) ─────────────────────────────────────────────

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(CHARS[((n >> 18) & 63) as usize] as char);
        out.push(CHARS[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            CHARS[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            CHARS[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    fn val(c: u8) -> Result<u32, String> {
        match c {
            b'A'..=b'Z' => Ok((c - b'A') as u32),
            b'a'..=b'z' => Ok((c - b'a' + 26) as u32),
            b'0'..=b'9' => Ok((c - b'0' + 52) as u32),
            b'+' => Ok(62),
            b'/' => Ok(63),
            b'=' => Ok(0),
            _ => Err(format!("Invalid base64 char: {c}")),
        }
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        if chunk.len() < 4 {
            break;
        }
        let a = val(chunk[0])?;
        let b = val(chunk[1])?;
        let c = val(chunk[2])?;
        let d = val(chunk[3])?;
        let n = (a << 18) | (b << 12) | (c << 6) | d;
        out.push(((n >> 16) & 0xFF) as u8);
        if chunk[2] != b'=' {
            out.push(((n >> 8) & 0xFF) as u8);
        }
        if chunk[3] != b'=' {
            out.push((n & 0xFF) as u8);
        }
    }
    Ok(out)
}
