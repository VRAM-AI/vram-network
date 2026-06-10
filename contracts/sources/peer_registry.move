// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

/// # PeerRegistry (v0.4)
///
/// Stores registered peers (miners and validators).
/// Peer credentials (R2 read keys) are stored as Seal IBE-encrypted ciphertext.
/// The plaintext never appears on-chain.

module slcl::peer_registry {
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;
    use sui::table::{Self, Table};
    use sui::clock::{Self, Clock};
    use std::string::String;

    const E_ALREADY_REGISTERED: u64 = 1;
    const E_NOT_REGISTERED: u64 = 2;
    const E_INSUFFICIENT_STAKE: u64 = 3;
    const E_UNAUTHORIZED: u64 = 4;
    const E_INVALID_PEER_TYPE: u64 = 5;

    const PEER_TYPE_MINER: u8 = 0;
    const PEER_TYPE_VALIDATOR: u8 = 1;

    public struct PeerRegistry has key {
        id: UID,
        peers: Table<u64, PeerRecord>,
        address_to_uid: Table<address, u64>,
        next_uid: u64,
        min_miner_stake: u64,
        min_validator_stake: u64,
        admin: address,
    }

    public struct PeerRecord has store, drop {
        uid: u64,
        owner: address,
        peer_type: u8,
        stake: u64,
        registered_at_window: u64,
        is_active: bool,
        /// Seal IBE-encrypted R2 read credentials
        seal_encrypted_object: vector<u8>,
        /// Seal identity: [package_id][peer_uid_le_bytes]
        seal_identity: vector<u8>,
        /// Seal package ID
        seal_package_id: String,
        /// Key server object IDs (t-of-n)
        key_server_object_ids: vector<String>,
        /// Decryption threshold
        threshold: u8,
        /// R2 bucket name (public)
        bucket_name: String,
        /// Cloudflare account ID (public)
        account_id: String,
    }

    public struct RegistryAdminCap has key, store { id: UID }

    fun init(ctx: &mut TxContext) {
        let registry = PeerRegistry {
            id: object::new(ctx),
            peers: table::new(ctx),
            address_to_uid: table::new(ctx),
            next_uid: 0,
            min_miner_stake: 1_000_000_000,
            min_validator_stake: 10_000_000_000,
            admin: tx_context::sender(ctx),
        };
        transfer::share_object(registry);
        transfer::transfer(
            RegistryAdminCap { id: object::new(ctx) },
            tx_context::sender(ctx),
        );
    }

    /// Register as a miner or validator.
    ///
    /// The caller provides Seal-encrypted R2 read credentials.
    /// The encrypted blob is stored on-chain; only staked validators can decrypt.
    public entry fun register_peer(
        registry: &mut PeerRegistry,
        peer_type: u8,
        stake: u64,
        seal_encrypted_object: vector<u8>,
        seal_identity: vector<u8>,
        seal_package_id: String,
        key_server_object_ids: vector<String>,
        threshold: u8,
        bucket_name: String,
        account_id: String,
        clock: &Clock,
        ctx: &mut TxContext,
    ) {
        let sender = tx_context::sender(ctx);

        assert!(!table::contains(&registry.address_to_uid, sender), E_ALREADY_REGISTERED);

        let min_stake = if (peer_type == PEER_TYPE_MINER) {
            registry.min_miner_stake
        } else {
            registry.min_validator_stake
        };
        assert!(stake >= min_stake, E_INSUFFICIENT_STAKE);

        let uid = registry.next_uid;
        registry.next_uid = uid + 1;

        let window = clock::timestamp_ms(clock) / 600_000;

        let record = PeerRecord {
            uid,
            owner: sender,
            peer_type,
            stake,
            registered_at_window: window,
            is_active: true,
            seal_encrypted_object,
            seal_identity,
            seal_package_id,
            key_server_object_ids,
            threshold,
            bucket_name,
            account_id,
        };

        table::add(&mut registry.peers, uid, record);
        table::add(&mut registry.address_to_uid, sender, uid);
    }

    public fun get_uid(registry: &PeerRegistry, addr: address): u64 {
        assert!(table::contains(&registry.address_to_uid, addr), E_NOT_REGISTERED);
        *table::borrow(&registry.address_to_uid, addr)
    }

    public fun is_registered(registry: &PeerRegistry, addr: address): bool {
        table::contains(&registry.address_to_uid, addr)
    }

    public fun get_stake(registry: &PeerRegistry, uid: u64): u64 {
        assert!(table::contains(&registry.peers, uid), E_NOT_REGISTERED);
        table::borrow(&registry.peers, uid).stake
    }

    public fun get_encrypted_credentials(
        registry: &PeerRegistry,
        uid: u64,
    ): (vector<u8>, vector<u8>, String) {
        assert!(table::contains(&registry.peers, uid), E_NOT_REGISTERED);
        let record = table::borrow(&registry.peers, uid);
        (record.seal_encrypted_object, record.seal_identity, record.seal_package_id)
    }

    /// Update encrypted credentials (e.g., bucket key rotation).
    public entry fun update_credentials(
        registry: &mut PeerRegistry,
        seal_encrypted_object: vector<u8>,
        seal_identity: vector<u8>,
        ctx: &mut TxContext,
    ) {
        let sender = tx_context::sender(ctx);
        assert!(table::contains(&registry.address_to_uid, sender), E_NOT_REGISTERED);
        let uid = *table::borrow(&registry.address_to_uid, sender);
        let record = table::borrow_mut(&mut registry.peers, uid);
        record.seal_encrypted_object = seal_encrypted_object;
        record.seal_identity = seal_identity;
    }
}
