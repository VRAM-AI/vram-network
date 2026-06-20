// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

//! Chain client configuration loaded from environment variables.

use vramhub_core::VramhubError;

/// All on-chain object IDs and RPC configuration.
#[derive(Debug, Clone)]
pub struct ChainConfig {
    pub sui_rpc_url: String,
    pub wallet_mnemonic: String,
    /// Deployed package ID (hex)
    pub package_id: String,
    pub peer_registry_id: String,
    pub round_state_id: String,
    pub score_ledger_id: String,
    pub enclave_registry_id: String,
    pub hparams_id: String,
    pub reward_pool_id: String,
    /// TrainingJobBoard shared object ID (VRAMHUB_TRAINING_JOB_BOARD_ID)
    pub training_job_board_id: String,
    /// Seal key server on-chain object IDs (comma-separated)
    pub seal_key_server_ids: Vec<String>,
    pub seal_threshold: u8,
}

impl ChainConfig {
    pub fn from_env() -> Result<Self, VramhubError> {
        fn env(var: &str) -> Result<String, VramhubError> {
            std::env::var(var).map_err(|_| VramhubError::MissingEnvVar {
                var: var.to_string(),
            })
        }

        let seal_ids_raw = env("VRAMHUB_SEAL_KEY_SERVER_IDS").unwrap_or_default();
        let seal_key_server_ids: Vec<String> = seal_ids_raw
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Ok(Self {
            sui_rpc_url: env("VRAMHUB_SUI_RPC_URL")
                .unwrap_or_else(|_| "https://fullnode.devnet.sui.io:443".to_string()),
            wallet_mnemonic: env("VRAMHUB_WALLET_MNEMONIC")?,
            package_id: env("VRAMHUB_PACKAGE_ID")?,
            peer_registry_id: env("VRAMHUB_PEER_REGISTRY_ID")?,
            round_state_id: env("VRAMHUB_ROUND_STATE_ID")?,
            score_ledger_id: env("VRAMHUB_SCORE_LEDGER_ID")?,
            enclave_registry_id: env("VRAMHUB_ENCLAVE_REGISTRY_ID")?,
            hparams_id: env("VRAMHUB_HPARAMS_ID")?,
            reward_pool_id: env("VRAMHUB_REWARD_POOL_ID").unwrap_or_default(),
            training_job_board_id: env("VRAMHUB_TRAINING_JOB_BOARD_ID").unwrap_or_default(),
            seal_key_server_ids,
            seal_threshold: env("VRAMHUB_SEAL_THRESHOLD")
                .unwrap_or_else(|_| "2".to_string())
                .parse()
                .unwrap_or(2),
        })
    }
}
