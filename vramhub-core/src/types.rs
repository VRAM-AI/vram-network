// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

//! Core types used across all SLCL crates.
//!
//! Mirror the on-chain Move struct layouts.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type PeerId = u64;
pub type WindowId = u64;

/// Fixed-point score: stored on-chain as u64, scale = 1e9.
/// Example: 1_000_000_000 = 1.0
pub type FixedScore = u64;

/// R2 bucket write credentials - NEVER posted on-chain.
/// Used only locally by the peer that owns the bucket.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bucket {
    pub name: String,
    pub account_id: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub endpoint: Option<String>,
}

/// Seal-encrypted R2 read credentials posted on-chain.
///
/// Encryption uses Seal IBE with a policy that allows only registered
/// validators to decrypt via the `seal_approve` function in seal_policy.move.
///
/// Public fields (name, account_id, endpoint) are stored in plaintext
/// for discovery. Only the access keys are encrypted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedBucket {
    /// R2 bucket name - public, for discovery
    pub name: String,
    /// Cloudflare account ID - public
    pub account_id: String,
    /// Optional endpoint override - public
    pub endpoint: Option<String>,
    /// Seal encrypted object bytes (IBE ciphertext of BucketReadCredentials)
    pub seal_encrypted_object: Vec<u8>,
    /// Seal identity used for encryption: [package_id][peer_uid_bytes]
    pub seal_identity: Vec<u8>,
    /// Seal package ID (the package containing seal_policy.move)
    pub seal_package_id: String,
    /// Key server object IDs used for encryption (t-of-n threshold)
    pub key_server_object_ids: Vec<String>,
    /// Threshold: how many key servers needed for decryption
    pub threshold: u8,
}

/// The plaintext that gets Seal-encrypted before posting on-chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketReadCredentials {
    pub access_key_id: String,
    pub secret_access_key: String,
}

/// A registered peer (miner or validator).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub uid: PeerId,
    pub address: String,
    pub encrypted_bucket: EncryptedBucket,
    pub stake: u64,
    pub registered_at_window: WindowId,
    pub peer_type: PeerType,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PeerType {
    Miner,
    Validator,
}

/// A registered Nautilus enclave instance.
///
/// Created during one-time enclave registration (expensive attestation verification).
/// After registration, the enclave_pubkey is used for cheap ECDSA verification
/// on every score submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnclaveInfo {
    /// On-chain object ID of this enclave registration
    pub object_id: String,
    /// The enclave's ephemeral public key (generated at boot, never leaves enclave)
    pub enclave_pubkey: Vec<u8>,
    /// PCR0: SHA-384 hash of OS and boot environment (48 bytes)
    pub pcr0: Vec<u8>,
    /// PCR1: SHA-384 hash of application code (48 bytes)
    pub pcr1: Vec<u8>,
    /// PCR2: SHA-384 hash of runtime configuration (48 bytes)
    pub pcr2: Vec<u8>,
    /// Validator UID that owns this enclave
    pub validator_uid: PeerId,
    /// Whether this enclave is currently active
    pub is_active: bool,
}

/// Current state of a training round.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundState {
    pub window: WindowId,
    pub window_start_ms: u64,
    pub put_window_open_ms: u64,
    pub put_window_close_ms: u64,
    pub top_g_peers: Vec<PeerId>,
    pub aggregation_r2_path: Option<String>,
    /// SHA256 of the checkpoint bytes - anchored on-chain.
    /// The Nautilus enclave verifies gradient submissions against this hash.
    pub checkpoint_hash: Option<[u8; 32]>,
    pub checkpoint_r2_path: Option<String>,
    pub is_finalized: bool,
}

/// Per-peer score state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerScore {
    pub uid: PeerId,
    pub openskill_mu: FixedScore,
    pub openskill_sigma: FixedScore,
    pub mu_generalization: i64,
    pub peer_score: FixedScore,
    pub normalized_weight: FixedScore,
    pub last_updated_window: WindowId,
}

/// Score submission from a validator, signed by the Nautilus enclave.
///
/// The chain verifies the Ed25519 signature against the registered enclave pubkey.
/// This is cheap (single sig verify) because attestation was verified at registration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreSubmission {
    pub validator_uid: PeerId,
    pub window: WindowId,
    /// uid -> score (fixed-point 9dp)
    pub scores: HashMap<PeerId, FixedScore>,
    pub stake_at_submission: u64,
    pub submitted_at_ms: u64,
    /// Ed25519 signature over the canonical encoding of (window, checkpoint_hash, scores).
    /// Signed by the enclave's ephemeral private key.
    pub enclave_signature: Vec<u8>,
    /// The exact bytes that the enclave signed (serde_json of EnclaveSignedPayload).
    /// Passed verbatim to the Move submit_scores PTB as signed_payload_bytes.
    pub signed_payload_bytes: Vec<u8>,
    /// SHA256 of the checkpoint used in TEE evaluation.
    /// Must match the checkpoint_hash anchored in RoundState for this window.
    pub checkpoint_hash: [u8; 32],
    /// On-chain object ID of the registered enclave that produced this signature.
    pub enclave_object_id: String,
}

/// Payload that the enclave signs.
/// Both the enclave and the on-chain contract construct this identically
/// to verify the signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnclaveSignedPayload {
    pub window: WindowId,
    pub checkpoint_hash: [u8; 32],
    pub scores: HashMap<PeerId, FixedScore>,
    /// Timestamp (ms) to prevent replay
    pub timestamp_ms: u64,
}

/// Fast evaluation result (cheap, run on all peers).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FastEvalResult {
    pub uid: PeerId,
    pub window: WindowId,
    pub passed_liveness: bool,
    pub passed_format: bool,
    pub passed_sync: bool,
    pub sync_score: f32,
    pub phi: f32,
}

/// Loss evaluation result - computed INSIDE the Nautilus enclave.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LossEvalResult {
    pub uid: PeerId,
    pub window: WindowId,
    pub loss_before: f32,
    pub loss_after_random: f32,
    pub loss_after_assigned: f32,
    pub loss_score: f32,
    pub poc_signal: i8,
}

/// Per-peer reward for a completed window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerReward {
    pub uid: PeerId,
    pub window: WindowId,
    pub normalized_weight: f64,
    pub token_amount: u64,
}

/// Gradient file metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradientMeta {
    pub uid: PeerId,
    pub window: WindowId,
    pub version: u32,
    /// Canonical storage reference. For R2/local backends this is the object key
    /// (e.g. `"gradient-42-7-v1.pt"`). For Walrus this is the blob_id prefixed
    /// with `"walrus:"` (e.g. `"walrus:abc123…"`).
    ///
    /// DEPRECATED name: will be renamed to `storage_ref` in v1.1 with a serde
    /// alias for backward compatibility. Treat as an opaque reference.
    pub r2_key: String,
    pub content_hash: String,
    pub uploaded_at_ms: u64,
    pub size_bytes: u64,
}

impl GradientMeta {
    pub fn r2_key(uid: PeerId, window: WindowId, version: u32) -> String {
        format!("gradient-{window}-{uid}-v{version}.pt")
    }
    pub fn aggregation_key(window: WindowId) -> String {
        format!("aggregation-{window}.pt")
    }
    pub fn checkpoint_key(window: WindowId, version: u32) -> String {
        format!("checkpoint-{window}-v{version}.pt")
    }
}

/// On-chain hyperparameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hparams {
    pub window_duration_ms: u64,
    pub put_window_open_ms: u64,
    pub topk_compression: u32,
    pub top_g: u32,
    pub validator_offset: u32,
    pub min_miner_stake: u64,
    pub min_validator_stake: u64,
    pub openskill_beta: f64,
    pub openskill_tau: f64,
    pub gauntlet_gamma: f64,
    pub sync_threshold: u32,
    pub emission_per_window: u64,
    pub checkpoint_frequency: u32,
    /// Expected PCR values for the current approved enclave build (48 bytes each).
    /// All three must match for an enclave to be registered.
    /// Updated via governance when the enclave binary changes.
    pub expected_pcr0: Vec<u8>,
    pub expected_pcr1: Vec<u8>,
    pub expected_pcr2: Vec<u8>,
}

impl Default for Hparams {
    fn default() -> Self {
        Self {
            window_duration_ms: 600_000,
            put_window_open_ms: 480_000,
            topk_compression: 32,
            top_g: 15,
            validator_offset: 2,
            min_miner_stake: 1_000_000_000,
            min_validator_stake: 10_000_000_000,
            openskill_beta: 25.0 / 6.0,
            openskill_tau: 25.0 / 300.0,
            gauntlet_gamma: 0.99,
            sync_threshold: 3,
            emission_per_window: 1_000_000_000_000,
            checkpoint_frequency: 100,
            expected_pcr0: vec![0u8; 48], // set at deployment
            expected_pcr1: vec![0u8; 48],
            expected_pcr2: vec![0u8; 48],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradient_meta_r2_key_format() {
        let key = GradientMeta::r2_key(7, 42, 3);
        assert_eq!(key, "gradient-42-7-v3.pt");
    }

    #[test]
    fn gradient_meta_aggregation_key_format() {
        let key = GradientMeta::aggregation_key(100);
        assert_eq!(key, "aggregation-100.pt");
    }

    #[test]
    fn gradient_meta_checkpoint_key_format() {
        let key = GradientMeta::checkpoint_key(5, 2);
        assert_eq!(key, "checkpoint-5-v2.pt");
    }

    #[test]
    fn hparams_default_window_is_10_minutes() {
        let h = Hparams::default();
        assert_eq!(h.window_duration_ms, 600_000);
        assert_eq!(h.expected_pcr0.len(), 48);
    }

    #[test]
    fn hparams_default_put_window_open_before_close() {
        let h = Hparams::default();
        assert!(
            h.put_window_open_ms < h.window_duration_ms,
            "put window must open before window ends"
        );
    }

    #[test]
    fn hparams_default_validator_stake_exceeds_miner() {
        let h = Hparams::default();
        assert!(h.min_validator_stake > h.min_miner_stake);
    }

    #[test]
    fn fixed_score_scale_is_1e9() {
        use crate::constants::FIXED_POINT_SCALE;
        assert_eq!(FIXED_POINT_SCALE, 1_000_000_000u64);
    }

    #[test]
    fn bucket_equality() {
        let b = Bucket {
            name: "test".to_string(),
            account_id: "acc".to_string(),
            access_key_id: "key".to_string(),
            secret_access_key: "secret".to_string(),
            endpoint: None,
        };
        assert_eq!(b, b.clone());
    }
}
