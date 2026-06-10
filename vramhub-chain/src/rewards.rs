// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

//! RewardDistributor on-chain operations.
//!
//! Called by the aggregator after a window is finalized.
//! Distributes emissions proportional to normalized_weight from ScoreLedger.

use vramhub_core::PeerReward;

/// Distribute rewards for a completed window.
///
/// Called by the aggregator after confirm the window is finalized on-chain.
/// Builds a PTB that calls reward_distributor::distribute(window, uids, weights).
///
/// The actual distribution is handled by `SuiChainClient::distribute_rewards`
/// in client.rs to keep the signing and execution logic centralized.
///
/// This module provides the data-layer helpers (parsing reward events, etc.).
pub fn compute_rewards(
    window: u64,
    emission_per_window: u64,
    normalized_weights: &[(u64, f64)], // (uid, weight)
) -> Vec<PeerReward> {
    normalized_weights
        .iter()
        .map(|&(uid, weight)| PeerReward {
            uid,
            window,
            normalized_weight: weight,
            token_amount: (weight * emission_per_window as f64) as u64,
        })
        .collect()
}
