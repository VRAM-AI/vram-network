// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

//! # vramhub-chain
//!
//! Sui RPC client for the SLCL coordination layer.
//!
//! Provides `SuiChainClient` — one method per on-chain operation.
//! All chain logic lives here; other crates call through this module.

pub mod client;
pub mod config;
pub mod enclave_registry;
pub mod peer_registry;
pub mod rewards;
pub mod round_state;
pub mod score_ledger;
pub mod training_jobs;
pub mod transactions;
pub mod validator;

pub use training_jobs::{BoardConfig, JobInfo};

pub use client::SuiChainClient;
pub use config::ChainConfig;

#[cfg(test)]
mod integration_tests {
    use super::*;

    fn devnet_config() -> Option<ChainConfig> {
        // Only run if devnet env vars are set
        if std::env::var("VRAMHUB_WALLET_MNEMONIC").is_err() {
            return None;
        }
        Some(ChainConfig {
            sui_rpc_url: std::env::var("VRAMHUB_SUI_RPC_URL")
                .unwrap_or_else(|_| "https://fullnode.devnet.sui.io:443".to_string()),
            wallet_mnemonic: std::env::var("VRAMHUB_WALLET_MNEMONIC").unwrap(),
            package_id: std::env::var("VRAMHUB_PACKAGE_ID").unwrap_or_default(),
            peer_registry_id: std::env::var("VRAMHUB_PEER_REGISTRY_ID").unwrap_or_default(),
            round_state_id: std::env::var("VRAMHUB_ROUND_STATE_ID").unwrap_or_default(),
            score_ledger_id: std::env::var("VRAMHUB_SCORE_LEDGER_ID").unwrap_or_default(),
            enclave_registry_id: std::env::var("VRAMHUB_ENCLAVE_REGISTRY_ID").unwrap_or_default(),
            hparams_id: std::env::var("VRAMHUB_HPARAMS_ID").unwrap_or_default(),
            reward_pool_id: std::env::var("VRAMHUB_REWARD_POOL_ID").unwrap_or_default(),
            training_job_board_id: std::env::var("SLCL_TRAINING_JOB_BOARD_ID").unwrap_or_default(),
            seal_key_server_ids: vec![],
            seal_threshold: 2,
        })
    }

    #[tokio::test]
    async fn test_get_hparams_from_devnet() {
        let Some(config) = devnet_config() else {
            return;
        };
        let client = SuiChainClient::new(config).await.expect("client init");
        let hparams = client.get_hparams().await.expect("get_hparams");
        assert_eq!(hparams.window_duration_ms, 600_000);
        assert_eq!(hparams.top_g, 15);
        println!("hparams: {:?}", hparams);
    }

    #[tokio::test]
    async fn test_current_window_is_nonzero() {
        let Some(config) = devnet_config() else {
            return;
        };
        let client = SuiChainClient::new(config).await.expect("client init");
        let window = client.current_window().await.expect("current_window");
        println!("current window: {window}");
        // Window is timestamp_ms / window_duration_ms — devnet clock is real time
        assert!(window > 0, "window should be nonzero on devnet");
    }
}
