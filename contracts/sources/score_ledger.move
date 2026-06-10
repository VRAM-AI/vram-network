// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

/// # ScoreLedger (v0.5)
///
/// Verifies enclave Ed25519 signatures on score submissions.
///
/// v0.5 changes:
/// - checkpoint verification reads on-chain value from RoundState (not caller-supplied)
/// - events emitted on submit_scores for indexer observability

module slcl::score_ledger {
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;
    use sui::table::{Self, Table};
    use sui::vec_map::{Self, VecMap};
    use sui::ed25519;
    use sui::event;

    const E_SCORE_ALREADY_SUBMITTED: u64 = 1;
    const E_WINDOW_FINALIZED: u64 = 2;
    const E_SIGNATURE_INVALID: u64 = 3;
    const E_CHECKPOINT_MISMATCH: u64 = 4;
    const E_ENCLAVE_NOT_REGISTERED: u64 = 5;
    const E_CHECKPOINT_NOT_ANCHORED: u64 = 6;

    public struct ScoreLedger has key {
        id: UID,
        submissions: Table<u64, vector<ValidatorSubmission>>,
        scores: Table<u64, PeerScore>,
        admin: address,
    }

    public struct ValidatorSubmission has store, drop {
        validator_uid: u64,
        window: u64,
        scores: VecMap<u64, u64>,
        stake_at_submission: u64,
        submitted_at_ms: u64,
        enclave_signature: vector<u8>,
        checkpoint_hash: vector<u8>,
    }

    public struct PeerScore has store, copy, drop {
        uid: u64,
        openskill_mu: u64,
        openskill_sigma: u64,
        mu_generalization: u64,
        peer_score: u64,
        normalized_weight: u64,
        last_updated_window: u64,
    }

    public struct LedgerAdminCap has key, store { id: UID }

    /// Emitted when a validator submits scores for a window.
    public struct ScoresSubmitted has copy, drop {
        validator_uid: u64,
        window: u64,
        score_count: u64,
        checkpoint_hash: vector<u8>,
    }

    fun init(ctx: &mut TxContext) {
        let ledger = ScoreLedger {
            id: object::new(ctx),
            submissions: table::new(ctx),
            scores: table::new(ctx),
            admin: tx_context::sender(ctx),
        };
        transfer::share_object(ledger);
        transfer::transfer(
            LedgerAdminCap { id: object::new(ctx) },
            tx_context::sender(ctx),
        );
    }

    #[test_only]
    public fun init_for_testing(ctx: &mut TxContext) { init(ctx) }

    /// Submit scores with enclave Ed25519 signature.
    ///
    /// Verifies:
    /// 1. The enclave is registered in EnclaveRegistry
    /// 2. The Ed25519 signature is valid against the registered enclave pubkey
    /// 3. checkpoint_hash matches the value anchored in RoundState (not caller-supplied)
    public entry fun submit_scores(
        ledger: &mut ScoreLedger,
        enclave_registry: &slcl::enclave_registry::EnclaveRegistry,
        round_state: &slcl::round_state::RoundState,
        validator_uid: u64,
        window: u64,
        scores_keys: vector<u64>,
        scores_values: vector<u64>,
        stake_at_submission: u64,
        submitted_at_ms: u64,
        enclave_signature: vector<u8>,
        signed_payload_bytes: vector<u8>,
        checkpoint_hash: vector<u8>,
        ctx: &mut TxContext,
    ) {
        // 1. Check enclave is registered
        assert!(
            slcl::enclave_registry::is_registered(enclave_registry, validator_uid),
            E_ENCLAVE_NOT_REGISTERED
        );

        // 2. Verify Ed25519 signature against registered enclave pubkey
        let enclave_pubkey = slcl::enclave_registry::get_enclave_pubkey(
            enclave_registry,
            validator_uid,
        );
        assert!(
            ed25519::ed25519_verify(
                &enclave_signature,
                &enclave_pubkey,
                &signed_payload_bytes,
            ),
            E_SIGNATURE_INVALID
        );

        // 3. Verify checkpoint_hash matches the value anchored on-chain in RoundState
        let anchored = slcl::round_state::get_checkpoint_hash(round_state, window);
        assert!(std::option::is_some(&anchored), E_CHECKPOINT_NOT_ANCHORED);
        assert!(
            checkpoint_hash == *std::option::borrow(&anchored),
            E_CHECKPOINT_MISMATCH
        );

        // 4. Record submission
        let score_count = vector::length(&scores_keys);
        let mut scores = vec_map::empty<u64, u64>();
        let mut i = 0u64;
        while (i < score_count) {
            vec_map::insert(
                &mut scores,
                *vector::borrow(&scores_keys, i),
                *vector::borrow(&scores_values, i),
            );
            i = i + 1;
        };

        let submission = ValidatorSubmission {
            validator_uid,
            window,
            scores,
            stake_at_submission,
            submitted_at_ms,
            enclave_signature,
            checkpoint_hash: checkpoint_hash,
        };

        if (!table::contains(&ledger.submissions, window)) {
            table::add(&mut ledger.submissions, window, vector::empty());
        };

        let existing = table::borrow(&ledger.submissions, window);
        let mut j = 0u64;
        while (j < vector::length(existing)) {
            assert!(
                vector::borrow(existing, j).validator_uid != validator_uid,
                E_SCORE_ALREADY_SUBMITTED,
            );
            j = j + 1;
        };

        vector::push_back(
            table::borrow_mut(&mut ledger.submissions, window),
            submission,
        );

        event::emit(ScoresSubmitted {
            validator_uid,
            window,
            score_count,
            checkpoint_hash,
        });

        let _ = ctx;
    }
}
