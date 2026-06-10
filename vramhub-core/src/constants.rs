// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

pub const PROTOCOL_VERSION: &str = "0.4.0";
pub const MIN_PEER_VERSION: &str = "0.4.0";
pub const MAX_MINERS: u64 = 256;
pub const MAX_VALIDATORS: u64 = 32;
pub const MAX_GRADIENT_SIZE_BYTES: u64 = 52_428_800;
pub const SYNC_SAMPLE_SIZE: usize = 2;
pub const FIXED_POINT_SCALE: u64 = 1_000_000_000;
pub const SCORE_SUBMISSION_TTL_WINDOWS: u64 = 3;
pub const FAST_EVAL_PENALTY: f32 = 0.75;
pub const MIN_VALIDATORS_FOR_FINALIZATION: usize = 1;
pub const DCT_CHUNK_SIZE: usize = 4096;
pub const DEFAULT_TOPK: usize = 32;
pub const QUANTIZATION_BITS: u8 = 2;

/// Default Seal threshold (2-of-3 key servers).
pub const DEFAULT_SEAL_THRESHOLD: u8 = 2;

/// Maximum time to wait for enclave evaluation response (ms).
pub const ENCLAVE_EVAL_TIMEOUT_MS: u64 = 120_000; // 2 minutes

/// Default enclave HTTP port (on the parent EC2 instance, forwarded via vsock).
pub const ENCLAVE_HTTP_PORT: u16 = 3000;
