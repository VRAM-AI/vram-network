// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2024-2025 VRAM AI Limited

//! Admin commands — require the EnclaveAdminCap owned by the deployer wallet.

use anyhow::{bail, Result};
use vramhub_chain::SuiChainClient;

/// Set the expected PCR0/1/2 values in EnclaveRegistry.
///
/// PCR values must be provided as lowercase hex strings (96 hex chars = 48 bytes each).
/// The signing wallet (VRAMHUB_WALLET_MNEMONIC) must own the EnclaveAdminCap.
pub async fn set_expected_pcrs(
    chain: &SuiChainClient,
    pcr0_hex: &str,
    pcr1_hex: &str,
    pcr2_hex: &str,
) -> Result<()> {
    let pcr0 = decode_pcr(pcr0_hex, 0)?;
    let pcr1 = decode_pcr(pcr1_hex, 1)?;
    let pcr2 = decode_pcr(pcr2_hex, 2)?;

    tracing::info!(
        pcr0 = pcr0_hex,
        pcr1 = pcr1_hex,
        pcr2 = pcr2_hex,
        "Setting expected PCR values in EnclaveRegistry"
    );

    chain.update_expected_pcrs(pcr0, pcr1, pcr2).await?;

    tracing::info!("EnclaveRegistry PCRs updated — validators can now register their enclave");
    println!("EnclaveRegistry PCRs set successfully.");
    println!("  PCR0: {pcr0_hex}");
    println!("  PCR1: {pcr1_hex}");
    println!("  PCR2: {pcr2_hex}");
    println!();
    println!("Next: on the validator node, run:");
    println!(
        "  vram-cli register-enclave --enclave-url http://localhost:3000 --validator-uid <UID>"
    );

    Ok(())
}

fn decode_pcr(hex_str: &str, index: u8) -> Result<Vec<u8>> {
    let bytes =
        hex::decode(hex_str.trim()).map_err(|e| anyhow::anyhow!("PCR{index}: invalid hex: {e}"))?;
    if bytes.len() != 48 {
        bail!(
            "PCR{index} must be 48 bytes (96 hex chars), got {} bytes",
            bytes.len()
        );
    }
    Ok(bytes)
}
