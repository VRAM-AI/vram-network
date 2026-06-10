// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

/// # EnclaveRegistry (v0.5)
///
/// Stores registered Nautilus enclave instances.
///
/// Registration is a one-time expensive operation.  In the interim
/// (until on-chain Nitro attestation verification is available via the
/// Sui framework), `register_enclave` requires an `EnclaveAdminCap`
/// so only the governance wallet can whitelist enclaves.  When Sui
/// exposes `verify_nitro_attestation`, the cap requirement is lifted
/// and any validator can self-register.

module slcl::enclave_registry {
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;
    use sui::table::{Self, Table};
    use sui::event;

    const E_ALREADY_REGISTERED: u64 = 1;
    const E_PCR_MISMATCH: u64 = 2;
    const E_ATTESTATION_INVALID: u64 = 3;
    const E_NOT_REGISTERED: u64 = 4;
    const E_UNAUTHORIZED: u64 = 5;

    public struct EnclaveRegistry has key {
        id: UID,
        enclaves: Table<u64, EnclaveRecord>,
        expected_pcr0: vector<u8>,
        expected_pcr1: vector<u8>,
        expected_pcr2: vector<u8>,
        admin: address,
    }

    public struct EnclaveRecord has store, drop {
        validator_uid: u64,
        owner: address,
        enclave_pubkey: vector<u8>,
        pcr0: vector<u8>,
        pcr1: vector<u8>,
        pcr2: vector<u8>,
        registered_at_ms: u64,
        is_active: bool,
    }

    public struct EnclaveAdminCap has key, store { id: UID }

    /// Emitted when an enclave is registered.
    public struct EnclaveRegistered has copy, drop {
        validator_uid: u64,
        owner: address,
        registered_at_ms: u64,
    }

    /// Emitted when an enclave is deactivated.
    public struct EnclaveDeactivated has copy, drop {
        validator_uid: u64,
        owner: address,
    }

    fun init(ctx: &mut TxContext) {
        let registry = EnclaveRegistry {
            id: object::new(ctx),
            enclaves: table::new(ctx),
            expected_pcr0: vector::empty(),
            expected_pcr1: vector::empty(),
            expected_pcr2: vector::empty(),
            admin: tx_context::sender(ctx),
        };
        transfer::share_object(registry);
        transfer::transfer(
            EnclaveAdminCap { id: object::new(ctx) },
            tx_context::sender(ctx),
        );
    }

    #[test_only]
    public fun init_for_testing(ctx: &mut TxContext) {
        init(ctx)
    }

    /// Register a new enclave instance.
    ///
    /// Interim: requires EnclaveAdminCap until Sui provides on-chain
    /// Nitro attestation verification.  PCRs must match expected values
    /// if they have been configured via update_expected_pcrs.
    public entry fun register_enclave(
        registry: &mut EnclaveRegistry,
        _cap: &EnclaveAdminCap,
        validator_uid: u64,
        attestation_document: vector<u8>,
        enclave_pubkey: vector<u8>,
        pcr0: vector<u8>,
        pcr1: vector<u8>,
        pcr2: vector<u8>,
        clock: &sui::clock::Clock,
        ctx: &mut TxContext,
    ) {
        // 1. Verify PCRs match expected values (skip if not yet configured).
        if (!vector::is_empty(&registry.expected_pcr0)) {
            assert!(pcr0 == registry.expected_pcr0, E_PCR_MISMATCH);
            assert!(pcr1 == registry.expected_pcr1, E_PCR_MISMATCH);
            assert!(pcr2 == registry.expected_pcr2, E_PCR_MISMATCH);
        };

        // 2. Attestation document is accepted as-is while on-chain Nitro verify
        //    is not yet available.  The EnclaveAdminCap gate above ensures only
        //    the governance wallet can submit registrations.
        let _ = attestation_document;

        // 3. Store the enclave record.
        let sender = tx_context::sender(ctx);
        let registered_at_ms = sui::clock::timestamp_ms(clock);

        if (table::contains(&registry.enclaves, validator_uid)) {
            table::remove(&mut registry.enclaves, validator_uid);
        };

        table::add(&mut registry.enclaves, validator_uid, EnclaveRecord {
            validator_uid,
            owner: sender,
            enclave_pubkey,
            pcr0,
            pcr1,
            pcr2,
            registered_at_ms,
            is_active: true,
        });

        event::emit(EnclaveRegistered { validator_uid, owner: sender, registered_at_ms });
    }

    public fun get_enclave_pubkey(
        registry: &EnclaveRegistry,
        validator_uid: u64,
    ): vector<u8> {
        assert!(table::contains(&registry.enclaves, validator_uid), E_NOT_REGISTERED);
        table::borrow(&registry.enclaves, validator_uid).enclave_pubkey
    }

    public fun is_registered(registry: &EnclaveRegistry, validator_uid: u64): bool {
        table::contains(&registry.enclaves, validator_uid) &&
        table::borrow(&registry.enclaves, validator_uid).is_active
    }

    public entry fun update_expected_pcrs(
        registry: &mut EnclaveRegistry,
        _cap: &EnclaveAdminCap,
        pcr0: vector<u8>,
        pcr1: vector<u8>,
        pcr2: vector<u8>,
    ) {
        registry.expected_pcr0 = pcr0;
        registry.expected_pcr1 = pcr1;
        registry.expected_pcr2 = pcr2;
    }

    public entry fun deactivate_enclave(
        registry: &mut EnclaveRegistry,
        validator_uid: u64,
        ctx: &mut TxContext,
    ) {
        assert!(table::contains(&registry.enclaves, validator_uid), E_NOT_REGISTERED);
        let record = table::borrow_mut(&mut registry.enclaves, validator_uid);
        assert!(record.owner == tx_context::sender(ctx), E_UNAUTHORIZED);
        record.is_active = false;
        event::emit(EnclaveDeactivated { validator_uid, owner: record.owner });
    }
}
