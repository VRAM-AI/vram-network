// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2024-2025 VRAM AI Limited

//! SLCL CLI - operator commands.

use anyhow::Result;
use clap::{Parser, Subcommand};
use vramhub_chain::{ChainConfig, SuiChainClient};

mod commands;

#[derive(Parser)]
#[command(name = "vramhub-cli", about = "SLCL operator CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum AdminCommands {
    /// Set the expected PCR0/1/2 values in EnclaveRegistry (governance op).
    ///
    /// Run this with VRAMHUB_WALLET_MNEMONIC set to the admin (deployer) wallet.
    /// PCR values come from `nitro-cli describe-enclaves` on the validator host.
    SetExpectedPcrs {
        /// PCR0: SHA-384 of the EIF image (96 hex chars)
        #[arg(long)]
        pcr0: String,
        /// PCR1: SHA-384 of the kernel/bootstrap (96 hex chars)
        #[arg(long)]
        pcr1: String,
        /// PCR2: SHA-384 of the application (96 hex chars)
        #[arg(long)]
        pcr2: String,
    },
}

#[derive(Subcommand)]
enum Commands {
    /// Admin operations (require EnclaveAdminCap in the signing wallet)
    Admin {
        #[command(subcommand)]
        command: AdminCommands,
    },
    /// Register a Nautilus enclave on-chain (one-time per enclave instance)
    RegisterEnclave {
        /// HTTP endpoint of the running enclave (e.g., http://1.2.3.4:3000)
        #[arg(long)]
        enclave_url: String,
        /// Validator UID (must already be registered in ValidatorRegistry)
        #[arg(long)]
        validator_uid: u64,
    },
    /// Register this wallet as a miner in PeerRegistry
    RegisterMiner {
        /// Stake in MIST (1 SUI = 1_000_000_000 MIST). Minimum 1_000_000_000.
        #[arg(long, default_value = "1000000000")]
        stake: u64,
        /// R2 bucket name that will hold this miner's gradients
        #[arg(long, default_value = "")]
        bucket: String,
        /// Cloudflare account ID for the bucket
        #[arg(long, default_value = "")]
        account_id: String,
    },
    /// Register this wallet as a validator in PeerRegistry
    RegisterValidator {
        /// Stake in MIST (1 SUI = 1_000_000_000 MIST). Minimum 10_000_000_000.
        #[arg(long, default_value = "10000000000")]
        stake: u64,
        /// R2 bucket name that will hold validator data
        #[arg(long, default_value = "")]
        bucket: String,
        /// Cloudflare account ID for the bucket
        #[arg(long, default_value = "")]
        account_id: String,
    },
    /// Show current chain status
    Status,
    /// Show peer scores for the current window
    Scores,
    /// Show checkpoint info for a window
    Checkpoint {
        #[arg(long)]
        window: Option<u64>,
    },
    /// Training job marketplace (permissionless, Vast.ai-style)
    Job {
        #[command(subcommand)]
        command: JobCommands,
    },
}

#[derive(Subcommand)]
enum JobCommands {
    /// Show board config and stats
    Board,
    /// List jobs (optionally filter by status: OPEN, CLAIMED, COMPLETED, SETTLED, DISPUTED, REFUNDED)
    List {
        #[arg(long)]
        status: Option<String>,
        #[arg(long, default_value = "50")]
        limit: usize,
    },
    /// Show details of a specific job
    Show { job_id: u64 },
    /// Estimate the price for a training job without posting it
    Price {
        #[arg(long)]
        model_params_m: u64,
        #[arg(long)]
        dataset_tokens_m: u64,
        #[arg(long, default_value = "1")]
        num_epochs: u32,
        /// FP32, FP16, BF16, or INT8
        #[arg(long, default_value = "BF16")]
        precision: String,
    },
    /// Post a new training job (escrows VRAM from your wallet)
    Post {
        /// Model size in millions of parameters (e.g. 7000 = 7B)
        #[arg(long)]
        model_params_m: u64,
        /// Dataset size in millions of tokens (e.g. 10000 = 10B)
        #[arg(long)]
        dataset_tokens_m: u64,
        #[arg(long, default_value = "1")]
        num_epochs: u32,
        #[arg(long, default_value = "32")]
        batch_size: u32,
        #[arg(long, default_value = "2048")]
        sequence_length: u32,
        /// FP32, FP16, BF16, or INT8
        #[arg(long, default_value = "BF16")]
        precision: String,
        /// Minimum GPU VRAM required in GB
        #[arg(long, default_value = "24")]
        min_gpu_vram_gb: u32,
        /// Walrus blob ID of the training dataset ("walrus:{blob_id}")
        #[arg(long)]
        dataset_blob_id: String,
        /// Optional base model Walrus blob ID (empty = train from scratch)
        #[arg(long, default_value = "")]
        base_model_blob_id: String,
        /// Job deadline as Unix timestamp in milliseconds
        #[arg(long)]
        deadline_ms: u64,
    },
    /// Claim an open job as a miner
    Claim {
        job_id: u64,
        /// Your registered miner UID (0 if not registered in PeerRegistry)
        #[arg(long, default_value = "0")]
        miner_uid: u64,
    },
    /// Submit completed job result (miner only)
    Complete {
        job_id: u64,
        /// Walrus blob ID of the trained model weights
        #[arg(long)]
        result_blob_id: String,
        /// SHA-256 hash of the uploaded result (hex)
        #[arg(long)]
        result_hash: String,
    },
    /// Withdraw payment after dispute window (miner only)
    Withdraw { job_id: u64 },
    /// Refund an expired or abandoned job (customer only)
    Refund { job_id: u64 },
    /// Cancel an unclaimed job before deadline (customer only)
    Cancel { job_id: u64 },
    /// Dispute a completed job's result within the dispute window (customer only)
    Dispute { job_id: u64 },
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    let chain_config = ChainConfig::from_env()?;
    let chain_client = SuiChainClient::new(chain_config).await?;

    // In dev mode (VRAMHUB_SKIP_SEAL=true) use empty credential bytes so
    // register-miner / register-validator work without a live Seal server.
    let skip_seal = std::env::var("VRAMHUB_SKIP_SEAL")
        .map(|v| v == "true")
        .unwrap_or(false);

    match cli.command {
        Commands::Admin { command } => match command {
            AdminCommands::SetExpectedPcrs { pcr0, pcr1, pcr2 } => {
                commands::admin::set_expected_pcrs(&chain_client, &pcr0, &pcr1, &pcr2).await?;
            }
        },
        Commands::RegisterEnclave {
            enclave_url,
            validator_uid,
        } => {
            commands::register::register_enclave(&chain_client, &enclave_url, validator_uid)
                .await?;
        }
        Commands::RegisterMiner {
            stake,
            bucket,
            account_id,
        } => {
            if !skip_seal {
                eprintln!(
                    "WARNING: VRAMHUB_SKIP_SEAL is not set. Seal-encrypted credential bytes \
                    will be empty. Set VRAMHUB_SKIP_SEAL=true for testnet or supply pre-encrypted \
                    bytes via a custom build."
                );
            }
            commands::register_peer::register_miner(
                &chain_client,
                stake,
                bucket,
                account_id,
                vec![],
                vec![],
            )
            .await?;
        }
        Commands::RegisterValidator {
            stake,
            bucket,
            account_id,
        } => {
            if !skip_seal {
                eprintln!(
                    "WARNING: VRAMHUB_SKIP_SEAL is not set. Seal-encrypted credential bytes \
                    will be empty. Set VRAMHUB_SKIP_SEAL=true for testnet or supply pre-encrypted \
                    bytes via a custom build."
                );
            }
            commands::register_peer::register_validator(
                &chain_client,
                stake,
                bucket,
                account_id,
                vec![],
                vec![],
            )
            .await?;
        }
        Commands::Status => {
            commands::status::show_status(&chain_client).await?;
        }
        Commands::Scores => {
            commands::scores::show_scores(&chain_client).await?;
        }
        Commands::Checkpoint { window } => {
            commands::checkpoint::show_checkpoint(&chain_client, window).await?;
        }
        Commands::Job { command } => match command {
            JobCommands::Board => {
                commands::training::show_board(&chain_client).await?;
            }
            JobCommands::List { status, limit } => {
                let filter = status
                    .as_deref()
                    .map(commands::training::parse_status_filter)
                    .transpose()?;
                commands::training::list_jobs(&chain_client, filter, limit).await?;
            }
            JobCommands::Show { job_id } => {
                commands::training::show_job(&chain_client, job_id).await?;
            }
            JobCommands::Price {
                model_params_m,
                dataset_tokens_m,
                num_epochs,
                precision,
            } => {
                let prec = commands::training::parse_precision(&precision)?;
                commands::training::estimate_price(
                    &chain_client,
                    model_params_m,
                    dataset_tokens_m,
                    num_epochs,
                    prec,
                )
                .await?;
            }
            JobCommands::Post {
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
            } => {
                let prec = commands::training::parse_precision(&precision)?;
                commands::training::post_job(
                    &chain_client,
                    model_params_m,
                    dataset_tokens_m,
                    num_epochs,
                    batch_size,
                    sequence_length,
                    prec,
                    min_gpu_vram_gb,
                    dataset_blob_id,
                    base_model_blob_id,
                    deadline_ms,
                )
                .await?;
            }
            JobCommands::Claim { job_id, miner_uid } => {
                commands::training::claim_job(&chain_client, job_id, miner_uid).await?;
            }
            JobCommands::Complete {
                job_id,
                result_blob_id,
                result_hash,
            } => {
                commands::training::complete_job(
                    &chain_client,
                    job_id,
                    result_blob_id,
                    result_hash,
                )
                .await?;
            }
            JobCommands::Withdraw { job_id } => {
                commands::training::withdraw_payment(&chain_client, job_id).await?;
            }
            JobCommands::Refund { job_id } => {
                commands::training::refund_job(&chain_client, job_id).await?;
            }
            JobCommands::Cancel { job_id } => {
                commands::training::cancel_job(&chain_client, job_id).await?;
            }
            JobCommands::Dispute { job_id } => {
                commands::training::dispute_result(&chain_client, job_id).await?;
            }
        },
    }

    Ok(())
}
