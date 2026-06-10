// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

//! PTB construction helpers for SLCL chain operations.
//!
//! These are thin wrappers that document the Move call signatures for each
//! operation. Actual PTB building + execution lives in SuiChainClient methods.
//!
//! Move function signatures this module targets:
//!
//! peer_registry::register_peer(registry, peer_type, stake, seal_obj, seal_id, bucket, account, ctx)
//! score_ledger::submit_scores(ledger, enc_registry, window, validator_uid, uids, scores, sig, checkpoint_hash, ctx)
//! round_state::anchor_checkpoint(round_state, window, r2_path, checkpoint_hash, ctx)
//! round_state::anchor_aggregation(round_state, window, r2_path, ctx)
//! enclave_registry::register_enclave(registry, uid, attestation, pubkey, pcr0, pcr1, pcr2, clock, ctx)
//! reward_distributor::distribute(pool, window, uids, amounts, ctx)
//! training_jobs::post_job(board, model_params_m, dataset_tokens_m, num_epochs, batch_size,
//!                         sequence_length, precision, min_gpu_vram_gb, dataset_blob_id,
//!                         base_model_blob_id, deadline_ms, payment, clock, ctx)
//! training_jobs::claim_job(board, job_id, miner_uid, clock, ctx)
//! training_jobs::complete_job(board, job_id, result_blob_id, result_hash, clock, ctx)
//! training_jobs::withdraw_payment(board, job_id, clock, ctx)
//! training_jobs::refund_job(board, job_id, clock, ctx)
//! training_jobs::cancel_job(board, job_id, ctx)
//! training_jobs::dispute_result(board, job_id, clock, ctx)

/// Move module names used in PTBs.
pub const MOD_PEER_REGISTRY: &str = "peer_registry";
pub const MOD_SCORE_LEDGER: &str = "score_ledger";
pub const MOD_ROUND_STATE: &str = "round_state";
pub const MOD_ENCLAVE_REGISTRY: &str = "enclave_registry";
pub const MOD_REWARD_DISTRIBUTOR: &str = "reward_distributor";

/// Move function names used in PTBs.
pub const FN_REGISTER_PEER: &str = "register_peer";
pub const FN_SUBMIT_SCORES: &str = "submit_scores";
pub const FN_ANCHOR_CHECKPOINT: &str = "anchor_checkpoint";
pub const FN_ANCHOR_AGGREGATION: &str = "anchor_aggregation";
pub const FN_REGISTER_ENCLAVE: &str = "register_enclave";
pub const FN_UPDATE_EXPECTED_PCRS: &str = "update_expected_pcrs";
pub const FN_DISTRIBUTE: &str = "distribute";

pub const MOD_TRAINING_JOBS: &str = "training_jobs";
pub const FN_POST_JOB: &str = "post_job";
pub const FN_CLAIM_JOB: &str = "claim_job";
pub const FN_COMPLETE_JOB: &str = "complete_job";
pub const FN_WITHDRAW_PAYMENT: &str = "withdraw_payment";
pub const FN_REFUND_JOB: &str = "refund_job";
pub const FN_CANCEL_JOB: &str = "cancel_job";
pub const FN_DISPUTE_RESULT: &str = "dispute_result";
