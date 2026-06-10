// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

/// # RoundState (v0.7)
///
/// Training window state management.
/// Supports both admin-gated anchoring (testnet) and Ed25519-verified
/// aggregator-enclave anchoring (mainnet / v0.7).

module slcl::round_state {
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;
    use sui::clock::{Self, Clock};
    use sui::table::{Self, Table};
    use sui::event;
    use std::string::String;
    use std::option::{Self, Option};

    const E_NOT_IN_PUT_WINDOW: u64 = 1;
    const E_UNAUTHORIZED:     u64 = 3;
    const E_NO_AGGREGATOR: u64 = 4;
    const E_BAD_SIGNATURE: u64 = 5;

    public struct CheckpointAnchored has copy, drop {
        window: u64,
        checkpoint_hash: vector<u8>,
        attested: bool,
    }

    public struct AggregationAnchored has copy, drop {
        window: u64,
        storage_path: std::string::String,
    }

    public struct RoundState has key {
        id: UID,
        window_duration_ms: u64,
        put_window_open_ms: u64,
        rounds: Table<u64, RoundRecord>,
        admin: address,
        /// Ed25519 public key of the aggregator enclave.
        /// Set by admin after aggregator enclave registers.
        /// Once set, anchor_checkpoint_attested can be called by anyone
        /// who produces a valid signature — no admin trust required.
        aggregator_pubkey: vector<u8>,
    }

    public struct RoundRecord has store, drop {
        window: u64,
        aggregation_r2_path: Option<String>,
        checkpoint_r2_path: Option<String>,
        checkpoint_hash: Option<vector<u8>>,
        top_g_peers: vector<u64>,
        is_finalized: bool,
    }

    fun init(ctx: &mut TxContext) {
        transfer::share_object(RoundState {
            id: object::new(ctx),
            window_duration_ms: 600_000,
            put_window_open_ms: 480_000,
            rounds: table::new(ctx),
            admin: tx_context::sender(ctx),
            aggregator_pubkey: vector::empty(),
        });
    }

    public fun current_window(state: &RoundState, clock: &Clock): u64 {
        clock::timestamp_ms(clock) / state.window_duration_ms
    }

    public fun is_in_put_window(state: &RoundState, window: u64, clock: &Clock): bool {
        let now_ms = clock::timestamp_ms(clock);
        let window_start = window * state.window_duration_ms;
        let put_open = window_start + state.put_window_open_ms;
        let put_close = window_start + state.window_duration_ms;
        now_ms >= put_open && now_ms < put_close
    }

    public entry fun anchor_checkpoint(
        state: &mut RoundState,
        window: u64,
        r2_path: String,
        checkpoint_hash: vector<u8>,
        ctx: &mut TxContext,
    ) {
        assert!(tx_context::sender(ctx) == state.admin, E_UNAUTHORIZED);
        ensure_round_exists(state, window);
        let record = table::borrow_mut(&mut state.rounds, window);
        record.checkpoint_r2_path = option::some(r2_path);
        record.checkpoint_hash = option::some(checkpoint_hash);
        event::emit(CheckpointAnchored { window, checkpoint_hash, attested: false });
    }

    /// Anchor a checkpoint with an Ed25519 signature from the aggregator enclave.
    ///
    /// No admin trust required — validity comes from the cryptographic signature.
    /// The aggregator enclave signs `checkpoint_hash` with the key registered via
    /// `set_aggregator_pubkey`. Any caller can submit this once the signature is valid.
    ///
    /// Marks the round as finalized; the admin-gated `anchor_checkpoint` is the
    /// testnet fallback when no aggregator enclave is running.
    public entry fun anchor_checkpoint_attested(
        state:           &mut RoundState,
        window:          u64,
        checkpoint_hash: vector<u8>,
        signature:       vector<u8>,
    ) {
        assert!(!vector::is_empty(&state.aggregator_pubkey), E_NO_AGGREGATOR);
        assert!(
            sui::ed25519::ed25519_verify(&signature, &state.aggregator_pubkey, &checkpoint_hash),
            E_BAD_SIGNATURE,
        );
        ensure_round_exists(state, window);
        let record = table::borrow_mut(&mut state.rounds, window);
        record.checkpoint_hash = std::option::some(checkpoint_hash);
        record.is_finalized = true;
        event::emit(CheckpointAnchored { window, checkpoint_hash, attested: true });
    }

    /// Set the aggregator enclave public key (admin-only, one-time setup).
    public entry fun set_aggregator_pubkey(
        state:  &mut RoundState,
        pubkey: vector<u8>,
        ctx:    &mut TxContext,
    ) {
        assert!(tx_context::sender(ctx) == state.admin, E_UNAUTHORIZED);
        state.aggregator_pubkey = pubkey;
    }

    public entry fun anchor_aggregation(
        state: &mut RoundState, window: u64, r2_path: String,
        _clock: &Clock, ctx: &mut TxContext,
    ) {
        assert!(tx_context::sender(ctx) == state.admin, E_UNAUTHORIZED);
        ensure_round_exists(state, window);
        table::borrow_mut(&mut state.rounds, window).aggregation_r2_path =
            option::some(r2_path);
        event::emit(AggregationAnchored { window, storage_path: r2_path });
    }

    /// Trustless aggregation anchor — signed by the aggregator enclave.
    ///
    /// The aggregator signs `bcs(window) || r2_path_bytes` with the key set via
    /// `set_aggregator_pubkey`. No admin key required on mainnet.
    public entry fun anchor_aggregation_attested(
        state:     &mut RoundState,
        window:    u64,
        r2_path:   String,
        signature: vector<u8>,
    ) {
        assert!(!vector::is_empty(&state.aggregator_pubkey), E_NO_AGGREGATOR);
        // Signed message: window (8 bytes LE) ++ r2_path bytes
        let mut msg = std::bcs::to_bytes(&window);
        vector::append(&mut msg, *std::string::as_bytes(&r2_path));
        assert!(
            sui::ed25519::ed25519_verify(&signature, &state.aggregator_pubkey, &msg),
            E_BAD_SIGNATURE,
        );
        ensure_round_exists(state, window);
        table::borrow_mut(&mut state.rounds, window).aggregation_r2_path =
            option::some(r2_path);
        event::emit(AggregationAnchored { window, storage_path: r2_path });
    }

    public entry fun set_top_g(
        state: &mut RoundState, window: u64, top_g_peers: vector<u64>,
        ctx: &mut TxContext,
    ) {
        assert!(tx_context::sender(ctx) == state.admin, E_UNAUTHORIZED);
        ensure_round_exists(state, window);
        table::borrow_mut(&mut state.rounds, window).top_g_peers = top_g_peers;
    }

    public fun get_checkpoint_hash(state: &RoundState, window: u64): Option<vector<u8>> {
        if (table::contains(&state.rounds, window)) {
            table::borrow(&state.rounds, window).checkpoint_hash
        } else {
            option::none()
        }
    }

    public fun get_top_g_peers(state: &RoundState, window: u64): vector<u64> {
        if (table::contains(&state.rounds, window)) {
            table::borrow(&state.rounds, window).top_g_peers
        } else {
            vector::empty()
        }
    }

    fun ensure_round_exists(state: &mut RoundState, window: u64) {
        if (!table::contains(&state.rounds, window)) {
            table::add(&mut state.rounds, window, RoundRecord {
                window,
                aggregation_r2_path: option::none(),
                checkpoint_r2_path: option::none(),
                checkpoint_hash: option::none(),
                top_g_peers: vector::empty(),
                is_finalized: false,
            });
        };
    }

    #[test_only]
    public fun init_for_testing(ctx: &mut TxContext) { init(ctx) }

    public fun is_finalized(state: &RoundState, window: u64): bool {
        table::contains(&state.rounds, window) &&
        table::borrow(&state.rounds, window).is_finalized
    }
}
