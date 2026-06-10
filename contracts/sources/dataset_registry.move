// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

/// # DatasetRegistry (v0.7)
///
/// On-chain registry of encrypted training datasets and deterministic shard assignment.
///
/// Clients register an encrypted dataset (shard count + manifest hash).
/// Validators call `assigned_shard` to determine which shard each miner trains on
/// per window — the result is deterministic from (dataset_id, miner_uid, window),
/// so any validator can independently verify the assignment without coordination.
///
/// Key delivery (Seal IBE) and cooldown tracking are off-chain concerns in v0.7.

module slcl::dataset_registry {
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;
    use sui::table::{Self, Table};

    const E_UNAUTHORIZED: u64 = 1;
    const E_NOT_FOUND:    u64 = 2;
    const E_INACTIVE:     u64 = 3;

    public struct DatasetRegistry has key {
        id: UID,
        datasets: Table<u64, Dataset>,
        next_id: u64,
    }

    public struct Dataset has store {
        id:            u64,
        /// SHA-256 of the plaintext manifest (shard URLs, sizes, order).
        manifest_hash: vector<u8>,
        /// Number of encrypted shards in this dataset.
        shard_count:   u64,
        /// Max % of shards any one miner can access (default 5).
        cooldown_pct:  u64,
        owner:         address,
        is_active:     bool,
    }

    fun init(ctx: &mut TxContext) {
        transfer::share_object(DatasetRegistry {
            id: object::new(ctx),
            datasets: table::new(ctx),
            next_id: 0,
        });
    }

    #[test_only]
    public fun init_for_testing(ctx: &mut TxContext) { init(ctx) }

    /// Register a new encrypted dataset.
    /// Returns nothing — the assigned dataset_id is next_id - 1 after the call.
    public entry fun register_dataset(
        registry:      &mut DatasetRegistry,
        manifest_hash: vector<u8>,
        shard_count:   u64,
        cooldown_pct:  u64,
        ctx:           &mut TxContext,
    ) {
        let id = registry.next_id;
        registry.next_id = id + 1;
        table::add(&mut registry.datasets, id, Dataset {
            id,
            manifest_hash,
            shard_count,
            cooldown_pct,
            owner: tx_context::sender(ctx),
            is_active: true,
        });
    }

    /// Deterministic shard assignment for a given (dataset, miner, window) triple.
    ///
    /// Uses XOR of the three identifiers modulo shard_count — cheap, deterministic,
    /// and unpredictable to miners who don't know the other miners' UIDs.
    /// Any validator can recompute this without coordination.
    public fun assigned_shard(
        registry:   &DatasetRegistry,
        dataset_id: u64,
        miner_uid:  u64,
        window:     u64,
    ): u64 {
        assert!(table::contains(&registry.datasets, dataset_id), E_NOT_FOUND);
        let dataset = table::borrow(&registry.datasets, dataset_id);
        assert!(dataset.is_active, E_INACTIVE);
        (miner_uid ^ window ^ dataset_id) % dataset.shard_count
    }

    /// Deactivate a dataset (client-initiated cleanup).
    public entry fun revoke_dataset(
        registry:   &mut DatasetRegistry,
        dataset_id: u64,
        ctx:        &mut TxContext,
    ) {
        assert!(table::contains(&registry.datasets, dataset_id), E_NOT_FOUND);
        let dataset = table::borrow_mut(&mut registry.datasets, dataset_id);
        assert!(dataset.owner == tx_context::sender(ctx), E_UNAUTHORIZED);
        dataset.is_active = false;
    }

    // ── Queries ────────────────────────────────────────────────────────────────

    public fun shard_count(registry: &DatasetRegistry, dataset_id: u64): u64 {
        assert!(table::contains(&registry.datasets, dataset_id), E_NOT_FOUND);
        table::borrow(&registry.datasets, dataset_id).shard_count
    }

    public fun manifest_hash(registry: &DatasetRegistry, dataset_id: u64): vector<u8> {
        assert!(table::contains(&registry.datasets, dataset_id), E_NOT_FOUND);
        table::borrow(&registry.datasets, dataset_id).manifest_hash
    }

    public fun cooldown_pct(registry: &DatasetRegistry, dataset_id: u64): u64 {
        assert!(table::contains(&registry.datasets, dataset_id), E_NOT_FOUND);
        table::borrow(&registry.datasets, dataset_id).cooldown_pct
    }

    public fun is_active(registry: &DatasetRegistry, dataset_id: u64): bool {
        table::contains(&registry.datasets, dataset_id) &&
        table::borrow(&registry.datasets, dataset_id).is_active
    }

    public fun next_id(registry: &DatasetRegistry): u64 {
        registry.next_id
    }
}
