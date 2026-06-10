// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

/// # GradientRegistry (v1.0)
///
/// On-chain gradient discovery for the Walrus storage backend.
///
/// Walrus uses content-addressed blob storage — there is no prefix listing.
/// This registry is the canonical index that lets validators and the aggregator
/// discover gradient storage refs for each training window without relying on
/// Cloudflare R2's prefix API.
///
/// ## Flow
///
/// 1. Miner uploads gradient to Walrus (or R2), gets a storage_ref back.
/// 2. Miner calls `submit_gradient` with the ref, content hash, and encryption kind.
/// 3. Validator calls `list_submissions` to enumerate all refs for a window.
/// 4. Aggregator calls `commit_window` after top-G selection to lock the window.
///
/// ## Storage ref format
///
/// - Walrus:  `b"walrus:{blob_id}"`
/// - R2/local: `b"gradient-{window}-{uid}-v{version}.pt"`
///
/// The encryption_kind byte mirrors `EncryptionMode::kind_byte()` in the Rust SDK:
/// 0 = none, 1 = AES-256-GCM, 2 = Seal IBE.

module slcl::gradient_registry {
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;
    use sui::clock::{Self, Clock};
    use sui::table::{Self, Table};

    // ── Error codes ───────────────────────────────────────────────────────────

    const E_WINDOW_NOT_FOUND:  u64 = 1;
    const E_WINDOW_COMMITTED:  u64 = 2;
    const E_UNAUTHORIZED:      u64 = 3;

    // ── Objects ───────────────────────────────────────────────────────────────

    /// Shared singleton registry. Created once at package publish.
    public struct GradientRegistry has key {
        id: UID,
        admin: address,
        windows: Table<u64, WindowSubmissions>,
    }

    /// Per-window submission bucket.
    public struct WindowSubmissions has store {
        submissions: vector<GradientSubmission>,
        committed: bool,
    }

    /// Immutable record of one miner's gradient for a window.
    public struct GradientSubmission has store, copy, drop {
        /// Address of the submitting miner.
        submitter: address,
        /// Numeric UID from the peer registry.
        miner_uid: u64,
        /// Opaque storage ref — `"walrus:{blob_id}"` or an R2 key (UTF-8 bytes).
        storage_ref: vector<u8>,
        /// SHA-256 of the (encrypted) payload, for integrity verification.
        content_hash: vector<u8>,
        submitted_at_ms: u64,
        /// 0 = none, 1 = AES-256-GCM, 2 = Seal IBE.
        encryption_kind: u8,
    }

    // ── Init ──────────────────────────────────────────────────────────────────

    fun init(ctx: &mut TxContext) {
        transfer::share_object(GradientRegistry {
            id: object::new(ctx),
            admin: tx_context::sender(ctx),
            windows: table::new(ctx),
        });
    }

    #[test_only]
    public fun init_for_testing(ctx: &mut TxContext) { init(ctx) }

    // ── Miner-callable ───────────────────────────────────────────────────────

    /// Submit a gradient storage ref for the current window.
    ///
    /// Creates the window bucket on first submission (lazy init).
    /// Aborts if the window has already been committed by the aggregator.
    public entry fun submit_gradient(
        registry:       &mut GradientRegistry,
        window:         u64,
        miner_uid:      u64,
        storage_ref:    vector<u8>,
        content_hash:   vector<u8>,
        encryption_kind: u8,
        clock:          &Clock,
        ctx:            &mut TxContext,
    ) {
        // Lazily create the window bucket on first submission.
        if (!table::contains(&registry.windows, window)) {
            table::add(&mut registry.windows, window, WindowSubmissions {
                submissions: vector::empty(),
                committed: false,
            });
        };

        let ws = table::borrow_mut(&mut registry.windows, window);
        assert!(!ws.committed, E_WINDOW_COMMITTED);

        vector::push_back(&mut ws.submissions, GradientSubmission {
            submitter: tx_context::sender(ctx),
            miner_uid,
            storage_ref,
            content_hash,
            submitted_at_ms: clock::timestamp_ms(clock),
            encryption_kind,
        });
    }

    // ── Aggregator-callable ───────────────────────────────────────────────────

    /// Lock a window against further submissions.
    ///
    /// Called by the aggregator after top-G gradient selection.
    /// Only the registry admin may call this.
    public entry fun commit_window(
        registry: &mut GradientRegistry,
        window:   u64,
        ctx:      &mut TxContext,
    ) {
        assert!(tx_context::sender(ctx) == registry.admin, E_UNAUTHORIZED);
        assert!(table::contains(&registry.windows, window), E_WINDOW_NOT_FOUND);
        let ws = table::borrow_mut(&mut registry.windows, window);
        ws.committed = true;
    }

    /// Update the admin address (e.g. to transfer admin to the aggregator enclave).
    public entry fun set_admin(
        registry:  &mut GradientRegistry,
        new_admin: address,
        ctx:       &mut TxContext,
    ) {
        assert!(tx_context::sender(ctx) == registry.admin, E_UNAUTHORIZED);
        registry.admin = new_admin;
    }

    // ── Read-only ─────────────────────────────────────────────────────────────

    /// Return all gradient submissions for a window.
    ///
    /// Returns an empty vector if the window has no submissions yet.
    public fun list_submissions(
        registry: &GradientRegistry,
        window:   u64,
    ): vector<GradientSubmission> {
        if (!table::contains(&registry.windows, window)) {
            return vector::empty()
        };
        table::borrow(&registry.windows, window).submissions
    }

    public fun is_committed(registry: &GradientRegistry, window: u64): bool {
        table::contains(&registry.windows, window) &&
        table::borrow(&registry.windows, window).committed
    }

    public fun submission_count(registry: &GradientRegistry, window: u64): u64 {
        if (!table::contains(&registry.windows, window)) {
            return 0
        };
        vector::length(&table::borrow(&registry.windows, window).submissions)
    }

    // ── GradientSubmission field accessors ───────────────────────────────────

    public fun sub_storage_ref(s: &GradientSubmission): vector<u8>    { s.storage_ref }
    public fun sub_miner_uid(s: &GradientSubmission): u64             { s.miner_uid }
    public fun sub_content_hash(s: &GradientSubmission): vector<u8>   { s.content_hash }
    public fun sub_encryption_kind(s: &GradientSubmission): u8        { s.encryption_kind }
    public fun sub_submitter(s: &GradientSubmission): address         { s.submitter }
    public fun sub_submitted_at_ms(s: &GradientSubmission): u64       { s.submitted_at_ms }
}
