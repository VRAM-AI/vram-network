// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

//! SuiChainClient - the single entry point for all on-chain operations.

use std::sync::Arc;

use shared_crypto::intent::{Intent, IntentMessage};
use sui_sdk::rpc_types::{
    SuiExecutionStatus, SuiObjectDataOptions, SuiObjectResponseQuery,
    SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponseOptions,
};
use sui_sdk::{SuiClient, SuiClientBuilder};
use sui_types::{
    base_types::{ObjectID, ObjectRef, SuiAddress},
    crypto::{Signature, SignatureScheme, SuiKeyPair},
    object::Owner,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{CallArg, ObjectArg, SharedObjectMutability, Transaction, TransactionData},
    transaction_driver_types::ExecuteTransactionRequestType,
    Identifier,
};

use super::ChainConfig;
use vramhub_core::{
    constants::FIXED_POINT_SCALE, Hparams, PeerInfo, PeerScore, PeerType, ScoreSubmission,
    VramhubError,
};

const GAS_BUDGET: u64 = 50_000_000; // 0.05 SUI

/// High-level client for SLCL on-chain operations.
///
/// Wraps the Sui SDK and exposes one method per on-chain operation.
/// All serialization, transaction building, and signing is done here.
pub struct SuiChainClient {
    pub(crate) sui_client: Arc<SuiClient>,
    pub(crate) keypair: SuiKeyPair,
    pub(crate) sender: SuiAddress,
    pub(crate) config: ChainConfig,
}

impl SuiChainClient {
    pub async fn new(config: ChainConfig) -> Result<Self, VramhubError> {
        let sui_client = SuiClientBuilder::default()
            .build(&config.sui_rpc_url)
            .await
            .map_err(|e| VramhubError::RpcError(e.to_string()))?;

        let (sender, keypair) = derive_keypair_from_mnemonic(&config.wallet_mnemonic)
            .map_err(|e| VramhubError::ConfigError(e.to_string()))?;

        Ok(Self {
            sui_client: Arc::new(sui_client),
            keypair,
            sender,
            config,
        })
    }

    /// Returns the wallet address this client is signing with.
    pub fn my_address(&self) -> String {
        format!("0x{}", hex::encode(self.sender.as_ref()))
    }

    /// Current training window number from on-chain Clock.
    pub async fn current_window(&self) -> Result<u64, VramhubError> {
        let hparams = self.get_hparams().await?;
        let clock_id = ObjectID::from_hex_literal(
            "0x0000000000000000000000000000000000000000000000000000000000000006",
        )
        .map_err(|e| VramhubError::ConfigError(e.to_string()))?;

        let resp = self
            .sui_client
            .read_api()
            .get_object_with_options(clock_id, SuiObjectDataOptions::new().with_bcs())
            .await
            .map_err(|e| VramhubError::RpcError(e.to_string()))?;

        // Clock BCS layout: [32 bytes UID] [8 bytes timestamp_ms as u64 LE]
        let bcs_bytes = extract_bcs_bytes(resp)?;
        if bcs_bytes.len() < 40 {
            return Err(VramhubError::RpcError(
                "Clock object BCS too short".to_string(),
            ));
        }
        let timestamp_ms = u64::from_le_bytes(bcs_bytes[32..40].try_into().unwrap());
        Ok(timestamp_ms / hparams.window_duration_ms)
    }

    /// Fetch all active peers from PeerRegistry.
    pub async fn fetch_peers(&self) -> Result<Vec<PeerInfo>, VramhubError> {
        let registry_id = object_id(&self.config.peer_registry_id)?;
        super::peer_registry::fetch_all_peers(&self.sui_client, registry_id).await
    }

    /// Register this node as a peer with Seal-encrypted credentials.
    ///
    /// # Security notes
    ///
    /// - `seal_encrypted_object` contains the peer's Cloudflare R2 credentials
    ///   (access key + secret key) encrypted with Seal IBE. Only staked validators
    ///   who pass `seal_approve` can decrypt them. The raw credentials are never
    ///   stored on-chain in plaintext.
    /// - When `VRAMHUB_SKIP_SEAL=true` (testnet default), empty bytes are passed and
    ///   the contract's `seal_approve` check is bypassed. This must be `false` in
    ///   production — otherwise any validator can access any miner's R2 bucket.
    /// - The `stake` parameter is measured in MIST (1 SUI = 1e9 MIST). The minimum
    ///   stake to register is defined in `hparams.move` (currently 1 SUI).
    ///
    /// Move signature:
    ///   register_peer(registry, peer_type, stake,
    ///                 seal_encrypted_object, seal_identity,
    ///                 seal_package_id, key_server_object_ids, threshold,
    ///                 bucket_name, account_id, clock, ctx)
    pub async fn register_peer(
        &self,
        peer_type: PeerType,
        stake: u64,
        seal_encrypted_object: Vec<u8>,
        seal_identity: Vec<u8>,
        bucket_name: String,
        account_id: String,
    ) -> Result<u64, VramhubError> {
        let package_id = object_id(&self.config.package_id)?;
        let registry_id = object_id(&self.config.peer_registry_id)?;
        let registry_ver = self.get_shared_object_version(registry_id).await?;

        // Clock is a well-known shared object at 0x6
        let clock_id = ObjectID::from_hex_literal(
            "0x0000000000000000000000000000000000000000000000000000000000000006",
        )
        .map_err(|e| VramhubError::ConfigError(e.to_string()))?;
        let clock_ver = self.get_shared_object_version(clock_id).await?;

        let peer_type_byte: u8 = match peer_type {
            PeerType::Miner => 0,
            PeerType::Validator => 1,
        };

        // Seal metadata from config
        let seal_package_id = self.config.package_id.clone();
        let key_server_ids = self.config.seal_key_server_ids.clone();
        let threshold = self.config.seal_threshold;

        fn pure<T: serde::Serialize>(
            b: &mut ProgrammableTransactionBuilder,
            v: T,
        ) -> Result<sui_types::transaction::Argument, VramhubError> {
            let bytes =
                bcs::to_bytes(&v).map_err(|e| VramhubError::SerializationError(e.to_string()))?;
            b.input(CallArg::Pure(bytes))
                .map_err(|e| VramhubError::Internal(e.to_string()))
        }

        let mut builder = ProgrammableTransactionBuilder::new();

        let registry_arg = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: registry_id,
                initial_shared_version: registry_ver,
                mutability: SharedObjectMutability::Mutable,
            }))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let clock_arg = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: clock_id,
                initial_shared_version: clock_ver,
                mutability: SharedObjectMutability::Immutable,
            }))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let peer_type_arg = pure(&mut builder, peer_type_byte)?;
        let stake_arg = pure(&mut builder, stake)?;
        let seal_obj_arg = pure(&mut builder, seal_encrypted_object)?;
        let seal_id_arg = pure(&mut builder, seal_identity)?;
        let seal_pkg_arg = pure(&mut builder, seal_package_id)?;
        let key_svr_arg = pure(&mut builder, key_server_ids)?;
        let threshold_arg = pure(&mut builder, threshold)?;
        let bucket_arg = pure(&mut builder, bucket_name)?;
        let account_arg = pure(&mut builder, account_id)?;

        builder.programmable_move_call(
            package_id,
            Identifier::new("peer_registry").map_err(|e| VramhubError::Internal(e.to_string()))?,
            Identifier::new("register_peer").map_err(|e| VramhubError::Internal(e.to_string()))?,
            vec![],
            vec![
                registry_arg,
                peer_type_arg,
                stake_arg,
                seal_obj_arg,
                seal_id_arg,
                seal_pkg_arg,
                key_svr_arg,
                threshold_arg,
                bucket_arg,
                account_arg,
                clock_arg,
            ],
        );

        let address = self.my_address();
        let registry_id = object_id(&self.config.peer_registry_id)?;

        match self.execute_ptb(builder).await {
            Ok(_) => {}
            Err(VramhubError::TransactionFailed { reason }) => {
                // Translate known Move abort codes into structured errors.
                if is_peer_registry_abort(&reason, 1) {
                    // E_ALREADY_REGISTERED — look up the existing peer record.
                    if let Ok(Some(existing)) = super::peer_registry::get_peer_by_address(
                        &self.sui_client,
                        registry_id,
                        &address,
                    )
                    .await
                    {
                        return Err(VramhubError::PeerAlreadyRegistered { uid: existing.uid });
                    }
                }
                return Err(VramhubError::TransactionFailed { reason });
            }
            Err(e) => return Err(e),
        }

        // Look up the assigned UID from the on-chain registry by wallet address.
        match super::peer_registry::get_peer_by_address(&self.sui_client, registry_id, &address)
            .await?
        {
            Some(peer) => {
                tracing::info!(uid = peer.uid, address, "Registration confirmed on-chain");
                Ok(peer.uid)
            }
            None => Err(VramhubError::Internal(
                "register_peer PTB succeeded but UID not found in registry — try again or check tx"
                    .to_string(),
            )),
        }
    }

    /// Submit TEE-signed scores on-chain.
    pub async fn submit_scores(&self, submission: ScoreSubmission) -> Result<(), VramhubError> {
        let package_id = object_id(&self.config.package_id)?;
        let ledger_id = object_id(&self.config.score_ledger_id)?;
        let ledger_version = self.get_shared_object_version(ledger_id).await?;
        let enc_reg_id = object_id(&self.config.enclave_registry_id)?;
        let enc_reg_version = self.get_shared_object_version(enc_reg_id).await?;
        let round_state_id = object_id(&self.config.round_state_id)?;
        let round_state_version = self.get_shared_object_version(round_state_id).await?;

        let mut builder = ProgrammableTransactionBuilder::new();

        let ledger_arg = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: ledger_id,
                initial_shared_version: ledger_version,
                mutability: SharedObjectMutability::Mutable,
            }))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let enc_reg_arg = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: enc_reg_id,
                initial_shared_version: enc_reg_version,
                mutability: SharedObjectMutability::Immutable,
            }))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let round_state_arg = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: round_state_id,
                initial_shared_version: round_state_version,
                mutability: SharedObjectMutability::Immutable,
            }))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let validator_uid_arg = builder
            .input(CallArg::Pure(
                bcs::to_bytes(&submission.validator_uid)
                    .map_err(|e| VramhubError::SerializationError(e.to_string()))?,
            ))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let window_arg = builder
            .input(CallArg::Pure(bcs::to_bytes(&submission.window).map_err(
                |e| VramhubError::SerializationError(e.to_string()),
            )?))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let (uids, scores): (Vec<u64>, Vec<u64>) = submission.scores.into_iter().unzip();

        let uids_arg = builder
            .input(CallArg::Pure(bcs::to_bytes(&uids).map_err(|e| {
                VramhubError::SerializationError(e.to_string())
            })?))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let scores_arg = builder
            .input(CallArg::Pure(bcs::to_bytes(&scores).map_err(|e| {
                VramhubError::SerializationError(e.to_string())
            })?))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let stake_arg = builder
            .input(CallArg::Pure(
                bcs::to_bytes(&submission.stake_at_submission)
                    .map_err(|e| VramhubError::SerializationError(e.to_string()))?,
            ))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let submitted_at_arg = builder
            .input(CallArg::Pure(
                bcs::to_bytes(&submission.submitted_at_ms)
                    .map_err(|e| VramhubError::SerializationError(e.to_string()))?,
            ))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let sig_arg = builder
            .input(CallArg::Pure(
                bcs::to_bytes(&submission.enclave_signature)
                    .map_err(|e| VramhubError::SerializationError(e.to_string()))?,
            ))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let signed_payload_arg = builder
            .input(CallArg::Pure(
                bcs::to_bytes(&submission.signed_payload_bytes)
                    .map_err(|e| VramhubError::SerializationError(e.to_string()))?,
            ))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let checkpoint_arg = builder
            .input(CallArg::Pure(
                bcs::to_bytes(&submission.checkpoint_hash.to_vec())
                    .map_err(|e| VramhubError::SerializationError(e.to_string()))?,
            ))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        builder.programmable_move_call(
            package_id,
            Identifier::new("score_ledger").map_err(|e| VramhubError::Internal(e.to_string()))?,
            Identifier::new("submit_scores").map_err(|e| VramhubError::Internal(e.to_string()))?,
            vec![],
            vec![
                ledger_arg,
                enc_reg_arg,
                round_state_arg,
                validator_uid_arg,
                window_arg,
                uids_arg,
                scores_arg,
                stake_arg,
                submitted_at_arg,
                sig_arg,
                signed_payload_arg,
                checkpoint_arg,
            ],
        );

        self.execute_ptb(builder).await?;
        Ok(())
    }

    /// Anchor a checkpoint hash for a window.
    pub async fn anchor_checkpoint(
        &self,
        window: u64,
        r2_path: String,
        checkpoint_hash: [u8; 32],
    ) -> Result<(), VramhubError> {
        let package_id = object_id(&self.config.package_id)?;
        let round_state_id = object_id(&self.config.round_state_id)?;
        let round_state_version = self.get_shared_object_version(round_state_id).await?;

        let mut builder = ProgrammableTransactionBuilder::new();

        let rs_arg = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: round_state_id,
                initial_shared_version: round_state_version,
                mutability: SharedObjectMutability::Mutable,
            }))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let window_arg = builder
            .input(CallArg::Pure(bcs::to_bytes(&window).map_err(|e| {
                VramhubError::SerializationError(e.to_string())
            })?))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let path_arg = builder
            .input(CallArg::Pure(bcs::to_bytes(&r2_path).map_err(|e| {
                VramhubError::SerializationError(e.to_string())
            })?))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let hash_arg = builder
            .input(CallArg::Pure(
                bcs::to_bytes(&checkpoint_hash.to_vec())
                    .map_err(|e| VramhubError::SerializationError(e.to_string()))?,
            ))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        builder.programmable_move_call(
            package_id,
            Identifier::new("round_state").map_err(|e| VramhubError::Internal(e.to_string()))?,
            Identifier::new("anchor_checkpoint")
                .map_err(|e| VramhubError::Internal(e.to_string()))?,
            vec![],
            vec![rs_arg, window_arg, path_arg, hash_arg],
        );

        self.execute_ptb(builder).await?;
        Ok(())
    }

    /// Anchor the aggregation storage path for a window.
    pub async fn anchor_aggregation(
        &self,
        window: u64,
        r2_path: String,
    ) -> Result<(), VramhubError> {
        let package_id = object_id(&self.config.package_id)?;
        let round_state_id = object_id(&self.config.round_state_id)?;
        let round_state_version = self.get_shared_object_version(round_state_id).await?;

        let clock_id = ObjectID::from_hex_literal(
            "0x0000000000000000000000000000000000000000000000000000000000000006",
        )
        .map_err(|e| VramhubError::ConfigError(e.to_string()))?;

        let mut builder = ProgrammableTransactionBuilder::new();

        let rs_arg = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: round_state_id,
                initial_shared_version: round_state_version,
                mutability: SharedObjectMutability::Mutable,
            }))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let window_arg = builder
            .input(CallArg::Pure(bcs::to_bytes(&window).map_err(|e| {
                VramhubError::SerializationError(e.to_string())
            })?))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let path_arg = builder
            .input(CallArg::Pure(bcs::to_bytes(&r2_path).map_err(|e| {
                VramhubError::SerializationError(e.to_string())
            })?))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let clock_arg = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: clock_id,
                initial_shared_version: 1u64.into(),
                mutability: SharedObjectMutability::Immutable,
            }))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        builder.programmable_move_call(
            package_id,
            Identifier::new("round_state").map_err(|e| VramhubError::Internal(e.to_string()))?,
            Identifier::new("anchor_aggregation")
                .map_err(|e| VramhubError::Internal(e.to_string()))?,
            vec![],
            vec![rs_arg, window_arg, path_arg, clock_arg],
        );

        self.execute_ptb(builder).await?;
        Ok(())
    }

    /// Register a Nautilus enclave on-chain (one-time, governance-gated).
    ///
    /// Requires the EnclaveAdminCap to be in the signing wallet.
    pub async fn register_enclave(
        &self,
        validator_uid: u64,
        attestation_document: Vec<u8>,
        enclave_pubkey: Vec<u8>,
        pcr0: Vec<u8>,
        pcr1: Vec<u8>,
        pcr2: Vec<u8>,
    ) -> Result<(), VramhubError> {
        let cap_ref = self.find_enclave_admin_cap().await?;

        let package_id = object_id(&self.config.package_id)?;
        let enc_reg_id = object_id(&self.config.enclave_registry_id)?;
        let enc_reg_version = self.get_shared_object_version(enc_reg_id).await?;

        let clock_id = ObjectID::from_hex_literal(
            "0x0000000000000000000000000000000000000000000000000000000000000006",
        )
        .map_err(|e| VramhubError::ConfigError(e.to_string()))?;

        let mut builder = ProgrammableTransactionBuilder::new();

        let reg_arg = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: enc_reg_id,
                initial_shared_version: enc_reg_version,
                mutability: SharedObjectMutability::Mutable,
            }))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let cap_arg = builder
            .input(CallArg::Object(ObjectArg::ImmOrOwnedObject(cap_ref)))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let clock_arg = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: clock_id,
                initial_shared_version: 1u64.into(),
                mutability: SharedObjectMutability::Immutable,
            }))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let uid_arg = builder
            .input(CallArg::Pure(bcs::to_bytes(&validator_uid).map_err(
                |e| VramhubError::SerializationError(e.to_string()),
            )?))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let attest_arg = builder
            .input(CallArg::Pure(
                bcs::to_bytes(&attestation_document)
                    .map_err(|e| VramhubError::SerializationError(e.to_string()))?,
            ))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let pubkey_arg = builder
            .input(CallArg::Pure(bcs::to_bytes(&enclave_pubkey).map_err(
                |e| VramhubError::SerializationError(e.to_string()),
            )?))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let pcr0_arg = builder
            .input(CallArg::Pure(bcs::to_bytes(&pcr0).map_err(|e| {
                VramhubError::SerializationError(e.to_string())
            })?))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let pcr1_arg = builder
            .input(CallArg::Pure(bcs::to_bytes(&pcr1).map_err(|e| {
                VramhubError::SerializationError(e.to_string())
            })?))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let pcr2_arg = builder
            .input(CallArg::Pure(bcs::to_bytes(&pcr2).map_err(|e| {
                VramhubError::SerializationError(e.to_string())
            })?))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        // Move sig: register_enclave(registry, _cap, validator_uid, attestation_document,
        //           enclave_pubkey, pcr0, pcr1, pcr2, clock, ctx)
        builder.programmable_move_call(
            package_id,
            Identifier::new("enclave_registry")
                .map_err(|e| VramhubError::Internal(e.to_string()))?,
            Identifier::new("register_enclave")
                .map_err(|e| VramhubError::Internal(e.to_string()))?,
            vec![],
            vec![
                reg_arg, cap_arg, uid_arg, attest_arg, pubkey_arg, pcr0_arg, pcr1_arg, pcr2_arg,
                clock_arg,
            ],
        );

        self.execute_ptb(builder).await?;
        Ok(())
    }

    /// Set the expected PCR values in EnclaveRegistry (admin operation).
    ///
    /// Requires the `EnclaveAdminCap` to be owned by the signing wallet.
    /// Automatically discovers the cap from the sender's owned objects.
    pub async fn update_expected_pcrs(
        &self,
        pcr0: Vec<u8>,
        pcr1: Vec<u8>,
        pcr2: Vec<u8>,
    ) -> Result<(), VramhubError> {
        let cap_ref = self.find_enclave_admin_cap().await?;

        let package_id = object_id(&self.config.package_id)?;
        let enc_reg_id = object_id(&self.config.enclave_registry_id)?;
        let enc_reg_version = self.get_shared_object_version(enc_reg_id).await?;

        let mut builder = ProgrammableTransactionBuilder::new();

        let reg_arg = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: enc_reg_id,
                initial_shared_version: enc_reg_version,
                mutability: SharedObjectMutability::Mutable,
            }))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let cap_arg = builder
            .input(CallArg::Object(ObjectArg::ImmOrOwnedObject(cap_ref)))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let pcr0_arg = builder
            .input(CallArg::Pure(bcs::to_bytes(&pcr0).map_err(|e| {
                VramhubError::SerializationError(e.to_string())
            })?))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let pcr1_arg = builder
            .input(CallArg::Pure(bcs::to_bytes(&pcr1).map_err(|e| {
                VramhubError::SerializationError(e.to_string())
            })?))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let pcr2_arg = builder
            .input(CallArg::Pure(bcs::to_bytes(&pcr2).map_err(|e| {
                VramhubError::SerializationError(e.to_string())
            })?))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        builder.programmable_move_call(
            package_id,
            Identifier::new("enclave_registry")
                .map_err(|e| VramhubError::Internal(e.to_string()))?,
            Identifier::new("update_expected_pcrs")
                .map_err(|e| VramhubError::Internal(e.to_string()))?,
            vec![],
            vec![reg_arg, cap_arg, pcr0_arg, pcr1_arg, pcr2_arg],
        );

        self.execute_ptb(builder).await?;
        Ok(())
    }

    /// Find the EnclaveAdminCap owned by this wallet.
    async fn find_enclave_admin_cap(&self) -> Result<ObjectRef, VramhubError> {
        let objects = self
            .sui_client
            .read_api()
            .get_owned_objects(
                self.sender,
                Some(SuiObjectResponseQuery {
                    filter: None,
                    options: Some(SuiObjectDataOptions::new().with_type()),
                }),
                None,
                Some(50),
            )
            .await
            .map_err(|e| VramhubError::RpcError(e.to_string()))?;

        for obj_resp in objects.data {
            if let Some(data) = obj_resp.data {
                let type_str = data
                    .type_
                    .as_ref()
                    .map(|t| t.to_string())
                    .unwrap_or_default();
                if type_str.contains("EnclaveAdminCap") {
                    return Ok((data.object_id, data.version, data.digest));
                }
            }
        }

        Err(VramhubError::Internal(format!(
            "EnclaveAdminCap not found in wallet {}. \
             Ensure VRAMHUB_WALLET_MNEMONIC is set to the admin wallet mnemonic.",
            self.my_address()
        )))
    }

    /// Read on-chain hyperparameters.
    pub async fn get_hparams(&self) -> Result<Hparams, VramhubError> {
        let hparams_id = object_id(&self.config.hparams_id)?;
        let resp = self
            .sui_client
            .read_api()
            .get_object_with_options(hparams_id, SuiObjectDataOptions::new().with_content())
            .await
            .map_err(|e| VramhubError::RpcError(e.to_string()))?;

        let data = resp.data.ok_or_else(|| VramhubError::ObjectNotFound {
            object_id: self.config.hparams_id.clone(),
        })?;

        let content = data
            .content
            .ok_or_else(|| VramhubError::RpcError("Hparams object has no content".to_string()))?;

        let json = serde_json::to_value(content)
            .map_err(|e| VramhubError::SerializationError(e.to_string()))?;

        parse_hparams_from_json(&json)
    }

    /// Get current OpenSkill ratings for a list of peers.
    pub async fn get_peer_scores(&self, uids: &[u64]) -> Result<Vec<PeerScore>, VramhubError> {
        let ledger_id = object_id(&self.config.score_ledger_id)?;
        super::score_ledger::get_peer_scores(&self.sui_client, ledger_id, uids).await
    }

    /// Get the checkpoint hash anchored for a window.
    pub async fn get_checkpoint_hash(&self, window: u64) -> Result<Option<[u8; 32]>, VramhubError> {
        let round_state_id = object_id(&self.config.round_state_id)?;
        super::round_state::get_checkpoint_hash(&self.sui_client, round_state_id, window).await
    }

    /// Get the top-G peers for a window (for aggregation).
    pub async fn get_top_g_peers(&self, window: u64) -> Result<Vec<u64>, VramhubError> {
        let round_state_id = object_id(&self.config.round_state_id)?;
        super::round_state::get_top_g_peers(&self.sui_client, round_state_id, window).await
    }

    // -------------------------------------------------------------------------
    // Training job operations
    // -------------------------------------------------------------------------

    /// Estimate the price for a training job without posting it.
    pub async fn compute_job_price(
        &self,
        model_params_m: u64,
        dataset_tokens_m: u64,
        num_epochs: u32,
        precision: u8,
    ) -> Result<(u64, u64), VramhubError> {
        let board_id = object_id(&self.config.training_job_board_id)?;
        let config = super::training_jobs::fetch_board_config(&self.sui_client, board_id).await?;
        Ok(config.compute_price(model_params_m, dataset_tokens_m, num_epochs, precision))
    }

    /// Fetch the TrainingJobBoard configuration.
    pub async fn get_job_board_config(
        &self,
    ) -> Result<super::training_jobs::BoardConfig, VramhubError> {
        let board_id = object_id(&self.config.training_job_board_id)?;
        super::training_jobs::fetch_board_config(&self.sui_client, board_id).await
    }

    /// Get a single training job by ID.
    pub async fn get_training_job(
        &self,
        job_id: u64,
    ) -> Result<super::training_jobs::JobInfo, VramhubError> {
        let board_id = object_id(&self.config.training_job_board_id)?;
        super::training_jobs::fetch_job(&self.sui_client, board_id, job_id).await
    }

    /// List training jobs (optionally filtered by status). Max `limit` jobs fetched.
    pub async fn list_training_jobs(
        &self,
        status_filter: Option<u8>,
        limit: usize,
    ) -> Result<Vec<super::training_jobs::JobInfo>, VramhubError> {
        let board_id = object_id(&self.config.training_job_board_id)?;
        super::training_jobs::list_jobs(&self.sui_client, board_id, status_filter, limit).await
    }

    /// Post a new training job. Finds a VRAM coin large enough to cover the cost.
    ///
    /// Move sig: post_job(board, model_params_m, dataset_tokens_m, num_epochs,
    ///           batch_size, sequence_length, precision, min_gpu_vram_gb,
    ///           dataset_blob_id, base_model_blob_id, deadline_ms, payment, clock, ctx)
    #[allow(clippy::too_many_arguments)]
    pub async fn post_training_job(
        &self,
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
    ) -> Result<u64, VramhubError> {
        let board_id = object_id(&self.config.training_job_board_id)?;
        let board_ver = self.get_shared_object_version(board_id).await?;
        let package_id = object_id(&self.config.package_id)?;

        let config = super::training_jobs::fetch_board_config(&self.sui_client, board_id).await?;
        let (miner_payout, protocol_fee) =
            config.compute_price(model_params_m, dataset_tokens_m, num_epochs, precision);
        let required = miner_payout + protocol_fee;

        let coin_ref = self.find_vram_coin(required).await?;

        let clock_id = ObjectID::from_hex_literal(
            "0x0000000000000000000000000000000000000000000000000000000000000006",
        )
        .map_err(|e| VramhubError::ConfigError(e.to_string()))?;

        let mut builder = ProgrammableTransactionBuilder::new();

        let board_arg = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: board_id,
                initial_shared_version: board_ver,
                mutability: SharedObjectMutability::Mutable,
            }))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let coin_arg = builder
            .input(CallArg::Object(ObjectArg::ImmOrOwnedObject(coin_ref)))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let clock_arg = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: clock_id,
                initial_shared_version: 1u64.into(),
                mutability: SharedObjectMutability::Immutable,
            }))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        fn pure_arg<T: serde::Serialize>(
            b: &mut ProgrammableTransactionBuilder,
            v: T,
        ) -> Result<sui_types::transaction::Argument, VramhubError> {
            b.input(CallArg::Pure(
                bcs::to_bytes(&v).map_err(|e| VramhubError::SerializationError(e.to_string()))?,
            ))
            .map_err(|e| VramhubError::Internal(e.to_string()))
        }

        let p_model = pure_arg(&mut builder, model_params_m)?;
        let p_tokens = pure_arg(&mut builder, dataset_tokens_m)?;
        let p_epochs = pure_arg(&mut builder, num_epochs)?;
        let p_batch = pure_arg(&mut builder, batch_size)?;
        let p_seqlen = pure_arg(&mut builder, sequence_length)?;
        let p_prec = pure_arg(&mut builder, precision)?;
        let p_vram = pure_arg(&mut builder, min_gpu_vram_gb)?;
        let p_dataset = pure_arg(&mut builder, dataset_blob_id)?;
        let p_base = pure_arg(&mut builder, base_model_blob_id)?;
        let p_dead = pure_arg(&mut builder, deadline_ms)?;

        builder.programmable_move_call(
            package_id,
            Identifier::new("training_jobs").map_err(|e| VramhubError::Internal(e.to_string()))?,
            Identifier::new("post_job").map_err(|e| VramhubError::Internal(e.to_string()))?,
            vec![],
            vec![
                board_arg, p_model, p_tokens, p_epochs, p_batch, p_seqlen, p_prec, p_vram,
                p_dataset, p_base, p_dead, coin_arg, clock_arg,
            ],
        );

        self.execute_ptb(builder).await?;

        // Return the newly assigned job_id (= old job_counter)
        let updated = super::training_jobs::fetch_board_config(&self.sui_client, board_id).await?;
        Ok(updated.job_counter.saturating_sub(1))
    }

    /// Claim an open training job as a miner.
    ///
    /// Move sig: claim_job(board, job_id, miner_uid, clock, ctx)
    pub async fn claim_training_job(
        &self,
        job_id: u64,
        miner_uid: u64,
    ) -> Result<(), VramhubError> {
        let board_id = object_id(&self.config.training_job_board_id)?;
        let board_ver = self.get_shared_object_version(board_id).await?;
        let package_id = object_id(&self.config.package_id)?;
        let clock_id = ObjectID::from_hex_literal(
            "0x0000000000000000000000000000000000000000000000000000000000000006",
        )
        .map_err(|e| VramhubError::ConfigError(e.to_string()))?;

        let mut builder = ProgrammableTransactionBuilder::new();

        let board_arg = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: board_id,
                initial_shared_version: board_ver,
                mutability: SharedObjectMutability::Mutable,
            }))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let clock_arg = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: clock_id,
                initial_shared_version: 1u64.into(),
                mutability: SharedObjectMutability::Immutable,
            }))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        fn pure_u64(
            b: &mut ProgrammableTransactionBuilder,
            v: u64,
        ) -> Result<sui_types::transaction::Argument, VramhubError> {
            b.input(CallArg::Pure(
                bcs::to_bytes(&v).map_err(|e| VramhubError::SerializationError(e.to_string()))?,
            ))
            .map_err(|e| VramhubError::Internal(e.to_string()))
        }

        let job_id_arg = pure_u64(&mut builder, job_id)?;
        let miner_uid_arg = pure_u64(&mut builder, miner_uid)?;

        builder.programmable_move_call(
            package_id,
            Identifier::new("training_jobs").map_err(|e| VramhubError::Internal(e.to_string()))?,
            Identifier::new("claim_job").map_err(|e| VramhubError::Internal(e.to_string()))?,
            vec![],
            vec![board_arg, job_id_arg, miner_uid_arg, clock_arg],
        );

        self.execute_ptb(builder).await?;
        Ok(())
    }

    /// Submit a completed job result.
    ///
    /// Move sig: complete_job(board, job_id, result_blob_id, result_hash, clock, ctx)
    pub async fn complete_training_job(
        &self,
        job_id: u64,
        result_blob_id: String,
        result_hash: Vec<u8>,
    ) -> Result<(), VramhubError> {
        let board_id = object_id(&self.config.training_job_board_id)?;
        let board_ver = self.get_shared_object_version(board_id).await?;
        let package_id = object_id(&self.config.package_id)?;
        let clock_id = ObjectID::from_hex_literal(
            "0x0000000000000000000000000000000000000000000000000000000000000006",
        )
        .map_err(|e| VramhubError::ConfigError(e.to_string()))?;

        let mut builder = ProgrammableTransactionBuilder::new();

        let board_arg = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: board_id,
                initial_shared_version: board_ver,
                mutability: SharedObjectMutability::Mutable,
            }))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let clock_arg = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: clock_id,
                initial_shared_version: 1u64.into(),
                mutability: SharedObjectMutability::Immutable,
            }))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        fn pure_arg<T: serde::Serialize>(
            b: &mut ProgrammableTransactionBuilder,
            v: T,
        ) -> Result<sui_types::transaction::Argument, VramhubError> {
            b.input(CallArg::Pure(
                bcs::to_bytes(&v).map_err(|e| VramhubError::SerializationError(e.to_string()))?,
            ))
            .map_err(|e| VramhubError::Internal(e.to_string()))
        }

        let job_arg = pure_arg(&mut builder, job_id)?;
        let blob_arg = pure_arg(&mut builder, result_blob_id)?;
        let hash_arg = pure_arg(&mut builder, result_hash)?;

        builder.programmable_move_call(
            package_id,
            Identifier::new("training_jobs").map_err(|e| VramhubError::Internal(e.to_string()))?,
            Identifier::new("complete_job").map_err(|e| VramhubError::Internal(e.to_string()))?,
            vec![],
            vec![board_arg, job_arg, blob_arg, hash_arg, clock_arg],
        );

        self.execute_ptb(builder).await?;
        Ok(())
    }

    /// Withdraw payment after the dispute window has passed.
    ///
    /// Move sig: withdraw_payment(board, job_id, clock, ctx)
    pub async fn withdraw_job_payment(&self, job_id: u64) -> Result<(), VramhubError> {
        self.training_job_clock_call("withdraw_payment", job_id)
            .await
    }

    /// Customer refunds a job that has passed its deadline without completion.
    ///
    /// Move sig: refund_job(board, job_id, clock, ctx)
    pub async fn refund_training_job(&self, job_id: u64) -> Result<(), VramhubError> {
        self.training_job_clock_call("refund_job", job_id).await
    }

    /// Customer cancels an unclaimed (STATUS_OPEN) job before deadline.
    ///
    /// Move sig: cancel_job(board, job_id, ctx)
    pub async fn cancel_training_job(&self, job_id: u64) -> Result<(), VramhubError> {
        let board_id = object_id(&self.config.training_job_board_id)?;
        let board_ver = self.get_shared_object_version(board_id).await?;
        let package_id = object_id(&self.config.package_id)?;

        let mut builder = ProgrammableTransactionBuilder::new();

        let board_arg = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: board_id,
                initial_shared_version: board_ver,
                mutability: SharedObjectMutability::Mutable,
            }))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let job_id_arg = builder
            .input(CallArg::Pure(bcs::to_bytes(&job_id).map_err(|e| {
                VramhubError::SerializationError(e.to_string())
            })?))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        builder.programmable_move_call(
            package_id,
            Identifier::new("training_jobs").map_err(|e| VramhubError::Internal(e.to_string()))?,
            Identifier::new("cancel_job").map_err(|e| VramhubError::Internal(e.to_string()))?,
            vec![],
            vec![board_arg, job_id_arg],
        );

        self.execute_ptb(builder).await?;
        Ok(())
    }

    /// Customer disputes a completed job within the dispute window.
    ///
    /// Move sig: dispute_result(board, job_id, clock, ctx)
    pub async fn dispute_training_result(&self, job_id: u64) -> Result<(), VramhubError> {
        self.training_job_clock_call("dispute_result", job_id).await
    }

    /// Shared PTB builder for training job operations that take (board, job_id, clock, ctx).
    async fn training_job_clock_call(
        &self,
        fn_name: &str,
        job_id: u64,
    ) -> Result<(), VramhubError> {
        let board_id = object_id(&self.config.training_job_board_id)?;
        let board_ver = self.get_shared_object_version(board_id).await?;
        let package_id = object_id(&self.config.package_id)?;
        let clock_id = ObjectID::from_hex_literal(
            "0x0000000000000000000000000000000000000000000000000000000000000006",
        )
        .map_err(|e| VramhubError::ConfigError(e.to_string()))?;

        let mut builder = ProgrammableTransactionBuilder::new();

        let board_arg = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: board_id,
                initial_shared_version: board_ver,
                mutability: SharedObjectMutability::Mutable,
            }))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let job_id_arg = builder
            .input(CallArg::Pure(bcs::to_bytes(&job_id).map_err(|e| {
                VramhubError::SerializationError(e.to_string())
            })?))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        let clock_arg = builder
            .input(CallArg::Object(ObjectArg::SharedObject {
                id: clock_id,
                initial_shared_version: 1u64.into(),
                mutability: SharedObjectMutability::Immutable,
            }))
            .map_err(|e| VramhubError::Internal(e.to_string()))?;

        builder.programmable_move_call(
            package_id,
            Identifier::new("training_jobs").map_err(|e| VramhubError::Internal(e.to_string()))?,
            Identifier::new(fn_name).map_err(|e| VramhubError::Internal(e.to_string()))?,
            vec![],
            vec![board_arg, job_id_arg, clock_arg],
        );

        self.execute_ptb(builder).await?;
        Ok(())
    }

    /// Find a VRAM_TOKEN coin owned by this wallet with at least `min_amount` balance.
    async fn find_vram_coin(&self, min_amount: u64) -> Result<ObjectRef, VramhubError> {
        let coin_type = format!("{}::vram_token::VRAM_TOKEN", self.config.package_id);

        let coins = self
            .sui_client
            .coin_read_api()
            .get_coins(self.sender, Some(coin_type), None, None)
            .await
            .map_err(|e| VramhubError::RpcError(e.to_string()))?;

        let largest = coins.data.iter().max_by_key(|c| c.balance);

        match largest {
            Some(c) if c.balance >= min_amount => Ok((c.coin_object_id, c.version, c.digest)),
            Some(c) => Err(VramhubError::InsufficientVramBalance {
                have: c.balance,
                need: min_amount,
            }),
            None => Err(VramhubError::InsufficientVramBalance {
                have: 0,
                need: min_amount,
            }),
        }
    }

    // -------------------------------------------------------------------------
    // Internal helpers
    // -------------------------------------------------------------------------

    /// Get the initial shared version for a shared object (required for PTB inputs).
    pub(crate) async fn get_shared_object_version(
        &self,
        id: ObjectID,
    ) -> Result<sui_types::base_types::SequenceNumber, VramhubError> {
        let resp = self
            .sui_client
            .read_api()
            .get_object_with_options(id, SuiObjectDataOptions::new().with_owner())
            .await
            .map_err(|e| VramhubError::RpcError(e.to_string()))?;

        let data = resp.data.ok_or_else(|| VramhubError::ObjectNotFound {
            object_id: id.to_string(),
        })?;

        match data.owner {
            Some(Owner::Shared {
                initial_shared_version,
                ..
            }) => Ok(initial_shared_version),
            _ => Err(VramhubError::RpcError(format!(
                "Object {} is not shared",
                id
            ))),
        }
    }

    /// Build, sign, and execute a PTB. Returns the transaction digest.
    pub(crate) async fn execute_ptb(
        &self,
        builder: ProgrammableTransactionBuilder,
    ) -> Result<String, VramhubError> {
        let pt = builder.finish();

        // Fetch a gas coin
        let coins = self
            .sui_client
            .coin_read_api()
            .get_coins(self.sender, None, None, None)
            .await
            .map_err(|e| VramhubError::RpcError(e.to_string()))?;

        let gas_coin = coins
            .data
            .into_iter()
            .next()
            .ok_or_else(|| VramhubError::RpcError("no SUI coins found for gas".to_string()))?;

        let gas_ref = (gas_coin.coin_object_id, gas_coin.version, gas_coin.digest);

        let gas_price = self
            .sui_client
            .read_api()
            .get_reference_gas_price()
            .await
            .map_err(|e| VramhubError::RpcError(e.to_string()))?;

        let tx_data = TransactionData::new_programmable(
            self.sender,
            vec![gas_ref],
            pt,
            GAS_BUDGET,
            gas_price,
        );

        // Sign
        let intent_msg = IntentMessage::new(Intent::sui_transaction(), tx_data.clone());
        let sig = Signature::new_secure(&intent_msg, &self.keypair);
        let tx = Transaction::from_data(tx_data, vec![sig]);

        // Submit
        let resp = self
            .sui_client
            .quorum_driver_api()
            .execute_transaction_block(
                tx,
                SuiTransactionBlockResponseOptions::new().with_effects(),
                Some(ExecuteTransactionRequestType::WaitForLocalExecution),
            )
            .await
            .map_err(|e| VramhubError::RpcError(e.to_string()))?;

        // Check execution status
        if let Some(effects) = &resp.effects {
            if let SuiExecutionStatus::Failure { error } = effects.status() {
                return Err(VramhubError::TransactionFailed {
                    reason: error.clone(),
                });
            }
        }

        Ok(resp.digest.to_string())
    }
}

// -------------------------------------------------------------------------
// Helpers
// -------------------------------------------------------------------------

/// Returns true when a Sui execution failure is a Move abort from `peer_registry`
/// with the given abort code.
fn is_peer_registry_abort(reason: &str, code: u64) -> bool {
    reason.contains("peer_registry") && reason.contains(&format!(", {})", code))
}

fn object_id(hex: &str) -> Result<ObjectID, VramhubError> {
    ObjectID::from_hex_literal(hex)
        .map_err(|e| VramhubError::ConfigError(format!("invalid object id {hex}: {e}")))
}

fn extract_bcs_bytes(resp: sui_sdk::rpc_types::SuiObjectResponse) -> Result<Vec<u8>, VramhubError> {
    use sui_sdk::rpc_types::{SuiRawData, SuiRawMoveObject};

    let data = resp
        .data
        .ok_or_else(|| VramhubError::RpcError("object not found".to_string()))?;
    let raw = data
        .bcs
        .ok_or_else(|| VramhubError::RpcError("BCS not available".to_string()))?;
    match raw {
        SuiRawData::MoveObject(SuiRawMoveObject { bcs_bytes, .. }) => Ok(bcs_bytes),
        _ => Err(VramhubError::RpcError(
            "expected MoveObject BCS".to_string(),
        )),
    }
}

/// Derives an Ed25519 keypair from a BIP-39 mnemonic phrase.
///
/// # Security notes
///
/// - The mnemonic is the **root secret** for this node. Whoever holds the mnemonic
///   controls all funds and can sign on-chain operations on behalf of this peer.
/// - Load the mnemonic exclusively from the `VRAMHUB_WALLET_MNEMONIC` environment
///   variable (set it via `.env` which is gitignored). Never hard-code it.
/// - In production, consider using a secrets manager (AWS Secrets Manager,
///   HashiCorp Vault) rather than a plaintext `.env` file.
/// - The phrase is never written to disk here; it lives only in process memory
///   for the duration of the keypair derivation call.
///
/// # Derivation path
///
/// Uses the Sui standard: SLIP-10 Ed25519 at m/44'/784'/0'/0'/0'
/// This is compatible with the Sui CLI (`sui keytool`) and all Sui wallets.
fn derive_keypair_from_mnemonic(phrase: &str) -> anyhow::Result<(SuiAddress, SuiKeyPair)> {
    use bip39::Mnemonic;

    let mnemonic = Mnemonic::parse(phrase)?;
    let seed = mnemonic.to_seed("");

    // SLIP-10 Ed25519 derivation at m/44'/784'/0'/0'/0'
    let derivation_path: bip32::DerivationPath = "m/44'/784'/0'/0'/0'".parse()?;
    sui_keys::key_derive::derive_key_pair_from_path(
        &seed,
        Some(derivation_path),
        &SignatureScheme::ED25519,
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}

fn parse_hparams_from_json(json: &serde_json::Value) -> Result<Hparams, VramhubError> {
    fn get_u64(obj: &serde_json::Value, field: &str) -> Result<u64, VramhubError> {
        obj["fields"][field]
            .as_str()
            .and_then(|s| s.parse::<u64>().ok())
            .or_else(|| obj["fields"][field].as_u64())
            .ok_or_else(|| VramhubError::SerializationError(format!("missing field: {field}")))
    }
    fn get_u32(obj: &serde_json::Value, field: &str) -> Result<u32, VramhubError> {
        get_u64(obj, field).map(|v| v as u32)
    }
    fn get_vec_u8(obj: &serde_json::Value, field: &str) -> Result<Vec<u8>, VramhubError> {
        let arr = &obj["fields"][field];
        serde_json::from_value(arr.clone())
            .map_err(|e| VramhubError::SerializationError(format!("{field}: {e}")))
    }

    // OpenSkill params are stored on-chain as fixed-point u64 (scale 1e9)
    // Field names use _fp suffix to indicate fixed-point encoding.
    let beta_fp = get_u64(json, "openskill_beta_fp")?;
    let tau_fp = get_u64(json, "openskill_tau_fp")?;
    let gamma_fp = get_u64(json, "gauntlet_gamma_fp")?;
    let scale = FIXED_POINT_SCALE as f64;

    Ok(Hparams {
        window_duration_ms: get_u64(json, "window_duration_ms")?,
        put_window_open_ms: get_u64(json, "put_window_open_ms")?,
        topk_compression: get_u32(json, "topk_compression")?,
        top_g: get_u32(json, "top_g")?,
        validator_offset: get_u32(json, "validator_offset")?,
        min_miner_stake: get_u64(json, "min_miner_stake")?,
        min_validator_stake: get_u64(json, "min_validator_stake")?,
        openskill_beta: beta_fp as f64 / scale,
        openskill_tau: tau_fp as f64 / scale,
        gauntlet_gamma: gamma_fp as f64 / scale,
        sync_threshold: get_u32(json, "sync_threshold")?,
        emission_per_window: get_u64(json, "emission_per_window")?,
        checkpoint_frequency: get_u32(json, "checkpoint_frequency")?,
        expected_pcr0: get_vec_u8(json, "expected_pcr0")?,
        expected_pcr1: get_vec_u8(json, "expected_pcr1")?,
        expected_pcr2: get_vec_u8(json, "expected_pcr2")?,
    })
}
