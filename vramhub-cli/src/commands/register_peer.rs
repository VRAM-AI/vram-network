// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2024-2025 VRAM AI Limited

//! `register-miner` and `register-validator` commands.
//!
//! Registers the calling wallet as a miner or validator in PeerRegistry.
//! The wallet address is derived from the mnemonic in the environment.
//!
//! In production, Seal-encrypted R2 credentials should be supplied.
//! When `VRAMHUB_SKIP_SEAL=true` (dev mode) empty credential bytes are used
//! so the command works without a live Seal key server.

use anyhow::Result;
use vramhub_chain::SuiChainClient;
use vramhub_core::{PeerType, VramhubError};

pub async fn register_miner(
    chain: &SuiChainClient,
    stake: u64,
    bucket_name: String,
    account_id: String,
    seal_encrypted_object: Vec<u8>,
    seal_identity: Vec<u8>,
) -> Result<u64> {
    let address = chain.my_address();
    tracing::info!(address, stake, bucket = %bucket_name, "Registering miner on-chain");

    let uid = match chain
        .register_peer(
            PeerType::Miner,
            stake,
            seal_encrypted_object,
            seal_identity,
            bucket_name,
            account_id,
        )
        .await
    {
        Ok(uid) => uid,
        Err(VramhubError::PeerAlreadyRegistered { uid }) => {
            println!("Already registered — nothing to do.");
            println!("  Address : {address}");
            println!("  UID     : {uid}");
            println!("  Next    : VRAMHUB_MINER_UID={uid} is already on-chain. Start the miner.");
            return Ok(uid);
        }
        Err(e) => return Err(e.into()),
    };

    println!("Miner registered!");
    println!("  Address : {address}");
    println!("  UID     : {uid}");
    println!("  Next    : set VRAMHUB_MINER_UID={uid} in your .env and start vramhub-miner");

    Ok(uid)
}

pub async fn register_validator(
    chain: &SuiChainClient,
    stake: u64,
    bucket_name: String,
    account_id: String,
    seal_encrypted_object: Vec<u8>,
    seal_identity: Vec<u8>,
) -> Result<u64> {
    let address = chain.my_address();
    tracing::info!(address, stake, bucket = %bucket_name, "Registering validator on-chain");

    let uid = match chain
        .register_peer(
            PeerType::Validator,
            stake,
            seal_encrypted_object,
            seal_identity,
            bucket_name,
            account_id,
        )
        .await
    {
        Ok(uid) => uid,
        Err(VramhubError::PeerAlreadyRegistered { uid }) => {
            println!("Already registered — nothing to do.");
            println!("  Address : {address}");
            println!("  UID     : {uid}");
            println!("  Next    : run  VRAMHUB_TEST_MODE=true ./vram-validator");
            return Ok(uid);
        }
        Err(e) => return Err(e.into()),
    };

    println!("Validator registered!");
    println!("  Address : {address}");
    println!("  UID     : {uid}");
    println!("  Next    : set VRAMHUB_VALIDATOR_UID={uid} in your .env, then run register-enclave");

    Ok(uid)
}
