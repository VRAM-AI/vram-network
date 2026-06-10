// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2024-2025 VRAM AI Limited

//! Training job marketplace CLI commands.
//!
//! Vast.ai-style permissionless training: customers post jobs with VRAM escrow,
//! miners claim and complete them, payment releases automatically after dispute window.

use anyhow::Result;
use vramhub_chain::{
    training_jobs::{
        BoardConfig, JobInfo, PRECISION_BF16, PRECISION_FP16, PRECISION_FP32, PRECISION_INT8,
        STATUS_CLAIMED, STATUS_COMPLETED, STATUS_DISPUTED, STATUS_OPEN, STATUS_REFUNDED,
        STATUS_SETTLED,
    },
    SuiChainClient,
};

// ── Price estimation ──────────────────────────────────────────────────────────

/// Estimate job price without posting. Reads live board config.
pub async fn estimate_price(
    chain: &SuiChainClient,
    model_params_m: u64,
    dataset_tokens_m: u64,
    num_epochs: u32,
    precision: u8,
) -> Result<()> {
    let (miner_payout, protocol_fee) = chain
        .compute_job_price(model_params_m, dataset_tokens_m, num_epochs, precision)
        .await?;

    let total = miner_payout + protocol_fee;
    println!("Price estimate");
    println!(
        "  Model size    : {}M params ({:.1}B)",
        model_params_m,
        model_params_m as f64 / 1000.0
    );
    println!(
        "  Dataset       : {}M tokens ({:.1}B)",
        dataset_tokens_m,
        dataset_tokens_m as f64 / 1000.0
    );
    println!("  Epochs        : {num_epochs}");
    println!("  Precision     : {}", precision_label(precision));
    println!();
    println!(
        "  Miner payout  : {} VRAM ({miner_payout} mist)",
        mist_to_vram(miner_payout)
    );
    println!(
        "  Protocol fee  : {} VRAM ({protocol_fee} mist)",
        mist_to_vram(protocol_fee)
    );
    println!(
        "  Total escrow  : {} VRAM ({total} mist)",
        mist_to_vram(total)
    );
    Ok(())
}

// ── Board info ────────────────────────────────────────────────────────────────

pub async fn show_board(chain: &SuiChainClient) -> Result<()> {
    let config = chain.get_job_board_config().await?;
    print_board_config(&config);
    Ok(())
}

fn print_board_config(c: &BoardConfig) {
    println!("TrainingJobBoard");
    println!("  Jobs posted       : {}", c.job_counter);
    println!("  Price/unit        : {} mist", c.price_per_unit);
    println!("  Min price         : {} VRAM", mist_to_vram(c.min_price));
    println!(
        "  Protocol fee      : {}%",
        c.protocol_fee_bps as f64 / 100.0
    );
    println!("  Dispute window    : {}h", c.dispute_window_ms / 3_600_000);
    println!(
        "  Fee vault balance : {} VRAM",
        mist_to_vram(c.fee_vault_balance)
    );
    println!("  Treasury          : {}", c.treasury);
    println!("  Admin             : {}", c.admin);
}

// ── List jobs ─────────────────────────────────────────────────────────────────

pub async fn list_jobs(
    chain: &SuiChainClient,
    status_filter: Option<u8>,
    limit: usize,
) -> Result<()> {
    let jobs = chain.list_training_jobs(status_filter, limit).await?;

    if jobs.is_empty() {
        println!("No jobs found.");
        return Ok(());
    }

    println!(
        "{:<6} {:<10} {:<12} {:<12} {:<5} {:<6} {:<12} {:<10} DATASET",
        "ID", "STATUS", "MODEL(B)", "DATASET(B)", "EP", "PREC", "PAYOUT(VRAM)", "DEADLINE",
    );
    println!("{}", "-".repeat(100));

    for job in &jobs {
        let deadline_h = job.deadline_ms / 1000 / 3600;
        let dataset_short = if job.dataset_blob_id.len() > 24 {
            format!("{}…", &job.dataset_blob_id[..24])
        } else {
            job.dataset_blob_id.clone()
        };
        println!(
            "{:<6} {:<10} {:<12.1} {:<12.1} {:<5} {:<6} {:<12} {:<10} {}",
            job.id,
            job.status_label(),
            job.model_params_m as f64 / 1000.0,
            job.dataset_tokens_m as f64 / 1000.0,
            job.num_epochs,
            job.precision_label(),
            mist_to_vram(job.miner_payout),
            format!("epoch {}", deadline_h),
            dataset_short,
        );
    }

    println!("\n{} job(s) listed.", jobs.len());
    Ok(())
}

// ── Show single job ───────────────────────────────────────────────────────────

pub async fn show_job(chain: &SuiChainClient, job_id: u64) -> Result<()> {
    let job = chain.get_training_job(job_id).await?;
    print_job(&job);
    Ok(())
}

fn print_job(job: &JobInfo) {
    println!("Job #{}", job.id);
    println!(
        "  Status          : {} ({})",
        job.status_label(),
        job.status
    );
    println!("  Customer        : {}", job.customer);
    println!();
    println!(
        "  Model size      : {}M params ({:.1}B)",
        job.model_params_m,
        job.model_params_m as f64 / 1000.0
    );
    println!(
        "  Dataset tokens  : {}M ({:.1}B)",
        job.dataset_tokens_m,
        job.dataset_tokens_m as f64 / 1000.0
    );
    println!("  Epochs          : {}", job.num_epochs);
    println!("  Batch size      : {}", job.batch_size);
    println!("  Sequence len    : {}", job.sequence_length);
    println!("  Precision       : {}", job.precision_label());
    println!("  Min GPU VRAM    : {}GB", job.min_gpu_vram_gb);
    println!();
    println!("  Dataset blob    : {}", job.dataset_blob_id);
    if !job.base_model_blob_id.is_empty() {
        println!("  Base model blob : {}", job.base_model_blob_id);
    }
    println!();
    println!(
        "  Miner payout    : {} VRAM ({} mist)",
        mist_to_vram(job.miner_payout),
        job.miner_payout
    );
    println!(
        "  Protocol fee    : {} VRAM ({} mist)",
        mist_to_vram(job.protocol_fee),
        job.protocol_fee
    );
    println!(
        "  Total escrow    : {} VRAM",
        mist_to_vram(job.total_cost())
    );
    println!();
    println!("  Posted at       : {}", ms_to_utc(job.posted_at_ms));
    println!("  Deadline        : {}", ms_to_utc(job.deadline_ms));
    if job.status >= STATUS_CLAIMED {
        println!("  Claimed at      : {}", ms_to_utc(job.claimed_at_ms));
        println!(
            "  Miner           : {}",
            job.miner_address.as_deref().unwrap_or("-")
        );
        println!("  Miner UID       : {}", job.miner_uid);
    }
    if job.status >= STATUS_COMPLETED {
        println!("  Completed at    : {}", ms_to_utc(job.completed_at_ms));
        println!("  Result blob     : {}", job.result_blob_id);
        if !job.result_hash.is_empty() {
            println!("  Result hash     : {}", hex::encode(&job.result_hash));
        }
    }
}

// ── Post job ──────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub async fn post_job(
    chain: &SuiChainClient,
    model_params_m: u64,
    dataset_tokens_m: u64,
    num_epochs: u32,
    batch_size: u32,
    sequence_length: u32,
    precision: u8,
    min_gpu_vram_gb: u32,
    dataset_blob_id: String,
    base_model_blob_id: String,
    deadline_ms: u64,
) -> Result<()> {
    // Show estimated cost before submitting
    let (miner_payout, protocol_fee) = chain
        .compute_job_price(model_params_m, dataset_tokens_m, num_epochs, precision)
        .await?;
    let total = miner_payout + protocol_fee;

    println!("Posting training job…");
    println!(
        "  Model size  : {}M params ({:.1}B)",
        model_params_m,
        model_params_m as f64 / 1000.0
    );
    println!(
        "  Dataset     : {}M tokens ({:.1}B)",
        dataset_tokens_m,
        dataset_tokens_m as f64 / 1000.0
    );
    println!("  Escrow      : {} VRAM", mist_to_vram(total));

    let job_id = chain
        .post_training_job(
            model_params_m,
            dataset_tokens_m,
            num_epochs,
            batch_size,
            sequence_length,
            precision,
            min_gpu_vram_gb,
            dataset_blob_id,
            base_model_blob_id,
            deadline_ms,
        )
        .await?;

    println!("\nJob posted!");
    println!("  Job ID  : {job_id}");
    println!(
        "  Escrow  : {} VRAM locked until completed or refunded",
        mist_to_vram(total)
    );
    println!("  Next    : miners can claim job #{job_id} via  vramhub-cli job claim {job_id}");
    Ok(())
}

// ── Claim job ─────────────────────────────────────────────────────────────────

pub async fn claim_job(chain: &SuiChainClient, job_id: u64, miner_uid: u64) -> Result<()> {
    let job = chain.get_training_job(job_id).await?;

    if job.status != STATUS_OPEN {
        anyhow::bail!(
            "Job #{job_id} is {} — only OPEN jobs can be claimed",
            job.status_label()
        );
    }

    println!("Claiming job #{job_id}…");
    println!("  Dataset   : {}", job.dataset_blob_id);
    println!(
        "  Payout    : {} VRAM on completion",
        mist_to_vram(job.miner_payout)
    );
    println!("  Deadline  : {}", ms_to_utc(job.deadline_ms));

    chain.claim_training_job(job_id, miner_uid).await?;

    println!("\nJob claimed!");
    println!(
        "  1. Download dataset: walrus get {}",
        job.dataset_blob_id.trim_start_matches("walrus:")
    );
    println!("  2. Train the model");
    println!("  3. Upload result:    walrus store result_weights.pt");
    println!("  4. Complete job:     vramhub-cli job complete {job_id} --result-blob-id walrus:<blob_id> --result-hash <sha256>");
    Ok(())
}

// ── Complete job ──────────────────────────────────────────────────────────────

pub async fn complete_job(
    chain: &SuiChainClient,
    job_id: u64,
    result_blob_id: String,
    result_hash_hex: String,
) -> Result<()> {
    let result_hash = hex::decode(&result_hash_hex)
        .map_err(|e| anyhow::anyhow!("Invalid result hash hex: {e}"))?;

    println!("Submitting job #{job_id} completion…");
    chain
        .complete_training_job(job_id, result_blob_id.clone(), result_hash)
        .await?;

    let config = chain.get_job_board_config().await?;
    let dispute_h = config.dispute_window_ms / 3_600_000;
    println!("\nJob completed!");
    println!("  Result blob     : {result_blob_id}");
    println!("  Dispute window  : {dispute_h}h — customer can dispute within this window");
    println!("  Withdraw after  : vramhub-cli job withdraw {job_id}  (after {dispute_h}h)");
    Ok(())
}

// ── Withdraw payment ──────────────────────────────────────────────────────────

pub async fn withdraw_payment(chain: &SuiChainClient, job_id: u64) -> Result<()> {
    let job = chain.get_training_job(job_id).await?;

    if job.status != STATUS_COMPLETED {
        anyhow::bail!(
            "Job #{job_id} is {} — only COMPLETED jobs can be withdrawn (status=2)",
            job.status_label()
        );
    }

    println!("Withdrawing payment for job #{job_id}…");
    chain.withdraw_job_payment(job_id).await?;

    println!("\nPayment withdrawn!");
    println!("  Received : {} VRAM", mist_to_vram(job.miner_payout));
    Ok(())
}

// ── Refund job ────────────────────────────────────────────────────────────────

pub async fn refund_job(chain: &SuiChainClient, job_id: u64) -> Result<()> {
    let job = chain.get_training_job(job_id).await?;

    match job.status {
        STATUS_OPEN | STATUS_CLAIMED => {}
        STATUS_REFUNDED => {
            println!("Job #{job_id} is already refunded.");
            return Ok(());
        }
        _ => anyhow::bail!(
            "Job #{job_id} is {} — cannot refund in this state",
            job.status_label()
        ),
    }

    println!("Requesting refund for job #{job_id}…");
    chain.refund_training_job(job_id).await?;

    println!("\nRefund issued!");
    println!("  Received : {} VRAM", mist_to_vram(job.total_cost()));
    Ok(())
}

// ── Cancel job ────────────────────────────────────────────────────────────────

pub async fn cancel_job(chain: &SuiChainClient, job_id: u64) -> Result<()> {
    let job = chain.get_training_job(job_id).await?;

    if job.status != STATUS_OPEN {
        anyhow::bail!(
            "Job #{job_id} is {} — only OPEN (unclaimed) jobs can be cancelled",
            job.status_label()
        );
    }

    println!("Cancelling job #{job_id}…");
    chain.cancel_training_job(job_id).await?;

    println!(
        "\nJob cancelled. {} VRAM returned.",
        mist_to_vram(job.total_cost())
    );
    Ok(())
}

// ── Dispute result ────────────────────────────────────────────────────────────

pub async fn dispute_result(chain: &SuiChainClient, job_id: u64) -> Result<()> {
    let job = chain.get_training_job(job_id).await?;

    if job.status != STATUS_COMPLETED {
        anyhow::bail!(
            "Job #{job_id} is {} — can only dispute COMPLETED jobs",
            job.status_label()
        );
    }

    println!("Flagging job #{job_id} as disputed…");
    chain.dispute_training_result(job_id).await?;

    println!("\nDispute filed!");
    println!("  Result blob : {}", job.result_blob_id);
    println!("  The admin will review and resolve the dispute.");
    println!("  Your VRAM is frozen until resolution.");
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn mist_to_vram(mist: u64) -> String {
    format!("{:.4}", mist as f64 / 1_000_000_000.0)
}

fn ms_to_utc(ms: u64) -> String {
    if ms == 0 {
        return "-".to_string();
    }
    // Simple seconds-since-epoch display; a real impl would use chrono.
    let secs = ms / 1000;
    format!("unix:{secs}")
}

pub fn parse_precision(s: &str) -> Result<u8> {
    match s.to_uppercase().as_str() {
        "FP32" => Ok(PRECISION_FP32),
        "FP16" => Ok(PRECISION_FP16),
        "BF16" => Ok(PRECISION_BF16),
        "INT8" => Ok(PRECISION_INT8),
        _ => anyhow::bail!("Unknown precision '{s}'. Use FP32, FP16, BF16, or INT8"),
    }
}

fn precision_label(p: u8) -> &'static str {
    match p {
        PRECISION_FP32 => "FP32",
        PRECISION_FP16 => "FP16",
        PRECISION_BF16 => "BF16",
        PRECISION_INT8 => "INT8",
        _ => "?",
    }
}

pub fn parse_status_filter(s: &str) -> Result<u8> {
    match s.to_uppercase().as_str() {
        "OPEN" => Ok(STATUS_OPEN),
        "CLAIMED" => Ok(STATUS_CLAIMED),
        "COMPLETED" => Ok(STATUS_COMPLETED),
        "SETTLED" => Ok(STATUS_SETTLED),
        "DISPUTED" => Ok(STATUS_DISPUTED),
        "REFUNDED" => Ok(STATUS_REFUNDED),
        _ => anyhow::bail!(
            "Unknown status '{s}'. Use OPEN, CLAIMED, COMPLETED, SETTLED, DISPUTED, or REFUNDED"
        ),
    }
}
