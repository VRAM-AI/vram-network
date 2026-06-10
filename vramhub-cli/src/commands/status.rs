// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2024-2025 VRAM AI Limited

//! `status` command.

use anyhow::Result;
use vramhub_chain::SuiChainClient;

pub async fn show_status(chain: &SuiChainClient) -> Result<()> {
    let window = chain.current_window().await?;
    let hparams = chain.get_hparams().await?;
    println!("Current window: {window}");
    println!("Window duration: {}s", hparams.window_duration_ms / 1000);
    println!("Emission per window: {}", hparams.emission_per_window);
    Ok(())
}
