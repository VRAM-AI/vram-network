// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

//! TrainingJobBoard on-chain queries and type definitions.

use serde::{Deserialize, Serialize};
use sui_sdk::SuiClient;
use sui_types::base_types::ObjectID;
use sui_types::dynamic_field::DynamicFieldName;

use vramhub_core::VramhubError;

// ── Mirrors of Move constants ─────────────────────────────────────────────────

pub const STATUS_OPEN: u8 = 0;
pub const STATUS_CLAIMED: u8 = 1;
pub const STATUS_COMPLETED: u8 = 2;
pub const STATUS_SETTLED: u8 = 3;
pub const STATUS_DISPUTED: u8 = 4;
pub const STATUS_REFUNDED: u8 = 5;

pub const PRECISION_FP32: u8 = 0;
pub const PRECISION_FP16: u8 = 1;
pub const PRECISION_BF16: u8 = 2;
pub const PRECISION_INT8: u8 = 3;

/// Parsed snapshot of a `TrainingJob` from chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobInfo {
    pub id: u64,
    pub customer: String,
    pub model_params_m: u64,
    pub dataset_tokens_m: u64,
    pub num_epochs: u32,
    pub batch_size: u32,
    pub sequence_length: u32,
    pub precision: u8,
    pub min_gpu_vram_gb: u32,
    pub dataset_blob_id: String,
    pub base_model_blob_id: String,
    pub miner_payout: u64,
    pub protocol_fee: u64,
    pub posted_at_ms: u64,
    pub deadline_ms: u64,
    pub claimed_at_ms: u64,
    pub completed_at_ms: u64,
    pub status: u8,
    pub miner_address: Option<String>,
    pub miner_uid: u64,
    pub result_blob_id: String,
    pub result_hash: Vec<u8>,
}

impl JobInfo {
    pub fn status_label(&self) -> &'static str {
        match self.status {
            STATUS_OPEN => "OPEN",
            STATUS_CLAIMED => "CLAIMED",
            STATUS_COMPLETED => "COMPLETED",
            STATUS_SETTLED => "SETTLED",
            STATUS_DISPUTED => "DISPUTED",
            STATUS_REFUNDED => "REFUNDED",
            _ => "UNKNOWN",
        }
    }

    pub fn precision_label(&self) -> &'static str {
        match self.precision {
            PRECISION_FP32 => "FP32",
            PRECISION_FP16 => "FP16",
            PRECISION_BF16 => "BF16",
            PRECISION_INT8 => "INT8",
            _ => "?",
        }
    }

    /// Total customer payment in mist (miner_payout + protocol_fee).
    pub fn total_cost(&self) -> u64 {
        self.miner_payout + self.protocol_fee
    }
}

/// Parsed snapshot of `TrainingJobBoard` configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardConfig {
    pub job_counter: u64,
    pub price_per_unit: u64,
    pub protocol_fee_bps: u64,
    pub min_price: u64,
    pub dispute_window_ms: u64,
    pub fee_vault_balance: u64,
    pub treasury: String,
    pub admin: String,
}

impl BoardConfig {
    /// Apply the same pricing formula as Move's `compute_price`.
    pub fn compute_price(
        &self,
        model_params_m: u64,
        dataset_tokens_m: u64,
        num_epochs: u32,
        precision: u8,
    ) -> (u64, u64) {
        let compute_units = model_params_m
            .saturating_mul(dataset_tokens_m)
            .saturating_mul(num_epochs as u64)
            / 1_000;

        let adjusted = if precision == PRECISION_INT8 {
            compute_units / 2
        } else {
            compute_units
        };

        let raw = adjusted.saturating_mul(self.price_per_unit);
        let miner_payout = raw.max(self.min_price);
        let protocol_fee = miner_payout * self.protocol_fee_bps / 10_000;
        (miner_payout, protocol_fee)
    }
}

// ── Chain queries ─────────────────────────────────────────────────────────────

/// Read the TrainingJobBoard config from chain.
pub async fn fetch_board_config(
    sui_client: &SuiClient,
    board_id: ObjectID,
) -> Result<BoardConfig, VramhubError> {
    use sui_sdk::rpc_types::SuiObjectDataOptions;

    let resp = sui_client
        .read_api()
        .get_object_with_options(board_id, SuiObjectDataOptions::new().with_content())
        .await
        .map_err(|e| VramhubError::RpcError(e.to_string()))?;

    let data = resp.data.ok_or_else(|| VramhubError::ObjectNotFound {
        object_id: board_id.to_string(),
    })?;

    let content = data
        .content
        .ok_or_else(|| VramhubError::RpcError("board has no content".to_string()))?;

    let json = serde_json::to_value(content)
        .map_err(|e| VramhubError::SerializationError(e.to_string()))?;

    let f = &json["fields"];

    fn u64_field(f: &serde_json::Value, key: &str) -> Result<u64, VramhubError> {
        f[key]
            .as_str()
            .and_then(|s| s.parse().ok())
            .or_else(|| f[key].as_u64())
            .ok_or_else(|| VramhubError::SerializationError(format!("missing field: {key}")))
    }

    // fee_vault is nested: { "fields": { "balance": "..." } }
    let fee_vault_balance = f["fee_vault"]["fields"]["balance"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .or_else(|| f["fee_vault"]["fields"]["balance"].as_u64())
        .unwrap_or(0);

    Ok(BoardConfig {
        job_counter: u64_field(f, "job_counter")?,
        price_per_unit: u64_field(f, "price_per_unit")?,
        protocol_fee_bps: u64_field(f, "protocol_fee_bps")?,
        min_price: u64_field(f, "min_price")?,
        dispute_window_ms: u64_field(f, "dispute_window_ms")?,
        fee_vault_balance,
        treasury: f["treasury"].as_str().unwrap_or("").to_string(),
        admin: f["admin"].as_str().unwrap_or("").to_string(),
    })
}

/// Read a single TrainingJob from the board's dynamic field table.
pub async fn fetch_job(
    sui_client: &SuiClient,
    board_id: ObjectID,
    job_id: u64,
) -> Result<JobInfo, VramhubError> {
    let field_name = DynamicFieldName {
        type_: "u64"
            .parse()
            .map_err(|e: _| VramhubError::Internal(format!("{e}")))?,
        value: serde_json::Value::String(job_id.to_string()),
    };

    let resp = sui_client
        .read_api()
        .get_dynamic_field_object(board_id, field_name)
        .await
        .map_err(|e| VramhubError::RpcError(e.to_string()))?;

    let data = resp
        .data
        .ok_or(VramhubError::TrainingJobNotFound { job_id })?;

    let content = data
        .content
        .ok_or_else(|| VramhubError::RpcError(format!("job {job_id} has no content")))?;

    let json = serde_json::to_value(content)
        .map_err(|e| VramhubError::SerializationError(e.to_string()))?;

    // Dynamic field value is nested under ["fields"]["value"]["fields"]
    let v = &json["fields"]["value"]["fields"];

    parse_job_from_json(v, job_id)
}

/// List jobs with an optional status filter. Iterates 0..job_counter (max `limit`).
pub async fn list_jobs(
    sui_client: &SuiClient,
    board_id: ObjectID,
    status_filter: Option<u8>,
    limit: usize,
) -> Result<Vec<JobInfo>, VramhubError> {
    let config = fetch_board_config(sui_client, board_id).await?;
    let count = config.job_counter.min(limit as u64);
    let mut results = Vec::new();

    for job_id in 0..count {
        match fetch_job(sui_client, board_id, job_id).await {
            Ok(job) => {
                if status_filter.is_none_or(|s| job.status == s) {
                    results.push(job);
                }
            }
            Err(VramhubError::TrainingJobNotFound { .. }) => continue,
            Err(e) => return Err(e),
        }
    }

    Ok(results)
}

// ── JSON parsing ──────────────────────────────────────────────────────────────

fn parse_job_from_json(v: &serde_json::Value, _job_id: u64) -> Result<JobInfo, VramhubError> {
    fn u64f(v: &serde_json::Value, k: &str) -> u64 {
        v[k].as_str()
            .and_then(|s| s.parse().ok())
            .or_else(|| v[k].as_u64())
            .unwrap_or(0)
    }
    fn u32f(v: &serde_json::Value, k: &str) -> u32 {
        u64f(v, k) as u32
    }
    fn u8f(v: &serde_json::Value, k: &str) -> u8 {
        u64f(v, k) as u8
    }
    fn strf(v: &serde_json::Value, k: &str) -> String {
        v[k].as_str().unwrap_or("").to_string()
    }

    // miner_address is Option<address> — Sui encodes as null or string
    let miner_address = if v["miner_address"].is_null() {
        None
    } else {
        v["miner_address"].as_str().map(|s| s.to_string())
    };

    // result_hash is vector<u8> — Sui encodes as array of numbers
    let result_hash: Vec<u8> = v["result_hash"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_u64().map(|n| n as u8))
                .collect()
        })
        .unwrap_or_default();

    Ok(JobInfo {
        id: u64f(v, "id"),
        customer: strf(v, "customer"),
        model_params_m: u64f(v, "model_params_m"),
        dataset_tokens_m: u64f(v, "dataset_tokens_m"),
        num_epochs: u32f(v, "num_epochs"),
        batch_size: u32f(v, "batch_size"),
        sequence_length: u32f(v, "sequence_length"),
        precision: u8f(v, "precision"),
        min_gpu_vram_gb: u32f(v, "min_gpu_vram_gb"),
        dataset_blob_id: strf(v, "dataset_blob_id"),
        base_model_blob_id: strf(v, "base_model_blob_id"),
        miner_payout: u64f(v, "miner_payout"),
        protocol_fee: u64f(v, "protocol_fee"),
        posted_at_ms: u64f(v, "posted_at_ms"),
        deadline_ms: u64f(v, "deadline_ms"),
        claimed_at_ms: u64f(v, "claimed_at_ms"),
        completed_at_ms: u64f(v, "completed_at_ms"),
        status: u8f(v, "status"),
        miner_address,
        miner_uid: u64f(v, "miner_uid"),
        result_blob_id: strf(v, "result_blob_id"),
        result_hash,
    })
}
