// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2024-2025 VRAM AI Limited

//! `scores` command.

use anyhow::Result;
use vramhub_chain::SuiChainClient;

pub async fn show_scores(chain: &SuiChainClient) -> Result<()> {
    let window = chain.current_window().await?;
    let peers = chain.fetch_peers().await?;

    if peers.is_empty() {
        println!("No peers registered yet.");
        return Ok(());
    }

    let uids: Vec<u64> = peers.iter().map(|p| p.uid).collect();
    let mut scores = chain.get_peer_scores(&uids).await?;

    // Sort by peer_score descending
    scores.sort_by_key(|b| std::cmp::Reverse(b.peer_score));

    let scale = vramhub_core::constants::FIXED_POINT_SCALE as f64;

    println!("Scores for window {window}:");
    println!(
        "{:<6} {:<10} {:<8} {:<8} {:<12} {:<8}",
        "UID", "Score", "Mu", "Sigma", "Weight", "Window"
    );
    println!("{}", "-".repeat(60));

    for s in &scores {
        println!(
            "{:<6} {:<10.6} {:<8.3} {:<8.3} {:<12.6} {:<8}",
            s.uid,
            s.peer_score as f64 / scale,
            s.openskill_mu as f64 / scale,
            s.openskill_sigma as f64 / scale,
            s.normalized_weight as f64 / scale,
            s.last_updated_window,
        );
    }

    Ok(())
}
