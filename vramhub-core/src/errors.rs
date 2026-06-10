// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

use thiserror::Error;

#[derive(Debug, Error)]
pub enum VramhubError {
    // Chain errors
    #[error("Sui RPC error: {0}")]
    RpcError(String),
    #[error("Transaction failed: {reason}")]
    TransactionFailed { reason: String },
    #[error("Object not found on chain: {object_id}")]
    ObjectNotFound { object_id: String },
    #[error("Insufficient stake: have {have}, need {need}")]
    InsufficientStake { have: u64, need: u64 },
    #[error("Peer already registered: uid={uid}")]
    PeerAlreadyRegistered { uid: u64 },
    #[error("Peer not registered: address={address}")]
    PeerNotRegistered { address: String },

    // Window / timing errors
    #[error("Outside put window: window={window}, now_ms={now_ms}")]
    OutsidePutWindow { window: u64, now_ms: u64 },
    #[error("Window already finalized: {window}")]
    WindowAlreadyFinalized { window: u64 },
    #[error("Score already submitted for window {window} by validator {validator_uid}")]
    ScoreAlreadySubmitted { window: u64, validator_uid: u64 },

    // Storage errors (generic)
    #[error("Storage upload failed: ref={storage_ref}, backend={backend}, reason={reason}")]
    StorageUploadFailed {
        storage_ref: String,
        backend: String,
        reason: String,
    },
    #[error("Storage download failed: ref={storage_ref}, backend={backend}, reason={reason}")]
    StorageDownloadFailed {
        storage_ref: String,
        backend: String,
        reason: String,
    },
    // Storage errors (R2-specific, preserved for backward compatibility)
    #[error("R2 upload failed: key={key}, reason={reason}")]
    R2UploadFailed { key: String, reason: String },
    #[error("R2 download failed: key={key}, reason={reason}")]
    R2DownloadFailed { key: String, reason: String },
    // Storage errors (Walrus-specific)
    #[error("Walrus upload failed: reason={reason}")]
    WalrusUploadFailed { reason: String },
    #[error("Walrus download failed: blob_id={blob_id}, reason={reason}")]
    WalrusDownloadFailed { blob_id: String, reason: String },
    #[error("Walrus response parse error: {reason}")]
    WalrusResponseError { reason: String },
    #[error("Gradient not found: uid={uid}, window={window}")]
    GradientNotFound { uid: u64, window: u64 },
    #[error("Checkpoint not found for window {window}")]
    CheckpointNotFound { window: u64 },
    // Seal storage
    #[error("Seal client unavailable — configure VRAMHUB_SEAL_ENABLED=true or use AES fallback")]
    SealClientUnavailable,
    #[error("AES encryption failed: {reason}")]
    AesEncryptionFailed { reason: String },
    #[error("AES decryption failed: {reason}")]
    AesDecryptionFailed { reason: String },

    // Validation errors
    #[error("Gradient format invalid: {reason}")]
    GradientFormatInvalid { reason: String },
    #[error("Sync score too high: peer={uid}, score={score}, threshold={threshold}")]
    SyncScoreTooHigh {
        uid: u64,
        score: f32,
        threshold: f32,
    },
    #[error("Content hash mismatch: expected={expected}, got={got}")]
    ContentHashMismatch { expected: String, got: String },

    // Seal errors
    #[error("Seal encryption failed: {reason}")]
    SealEncryptionFailed { reason: String },
    #[error("Seal decryption failed: {reason}")]
    SealDecryptionFailed { reason: String },
    #[error("Seal policy denied: caller not authorized by seal_approve")]
    SealPolicyDenied,
    #[error("Seal key server error: {endpoint}: {reason}")]
    SealKeyServerError { endpoint: String, reason: String },
    #[error("Seal threshold not met: got {got} of {needed} key shares")]
    SealThresholdNotMet { got: usize, needed: usize },

    // Enclave / TEE errors
    #[error("Enclave signature invalid: {reason}")]
    EnclaveSignatureInvalid { reason: String },
    #[error("Enclave not registered: {object_id}")]
    EnclaveNotRegistered { object_id: String },
    #[error("PCR mismatch: pcr{index} expected={expected}, got={got}")]
    PcrMismatch {
        index: u8,
        expected: String,
        got: String,
    },
    #[error("Enclave unreachable: {endpoint}: {reason}")]
    EnclaveUnreachable { endpoint: String, reason: String },
    #[error("Enclave evaluation timed out after {timeout_ms}ms")]
    EnclaveTimeout { timeout_ms: u64 },
    #[error("Checkpoint hash mismatch in enclave: expected={expected}, got={got}")]
    CheckpointHashMismatch { expected: String, got: String },

    // Config errors
    #[error("Missing environment variable: {var}")]
    MissingEnvVar { var: String },
    #[error("Config parse error: {0}")]
    ConfigError(String),

    // Training job errors
    #[error("Insufficient VRAM balance: have {have}, need {need} mist")]
    InsufficientVramBalance { have: u64, need: u64 },
    #[error("Training job not found: id={job_id}")]
    TrainingJobNotFound { job_id: u64 },
    #[error("Training job in wrong state: job_id={job_id}, status={status}")]
    TrainingJobWrongState { job_id: u64, status: u8 },

    // General
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Internal error: {0}")]
    Internal(String),
}
