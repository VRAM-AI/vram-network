// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2024-2025 VRAM AI Limited

//! `checkpoint` command.

use anyhow::Result;
use vramhub_chain::SuiChainClient;

pub async fn show_checkpoint(chain: &SuiChainClient, window: Option<u64>) -> Result<()> {
    let window = match window {
        Some(w) => w,
        None => chain.current_window().await?,
    };

    match chain.get_checkpoint_hash(window).await? {
        Some(hash) => println!("Window {window} checkpoint: {}", hex::encode(hash)),
        None => println!("Window {window}: no checkpoint anchored yet"),
    }
    Ok(())
}
