// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

#[test_only]
module slcl::enclave_registry_tests {
    use sui::test_scenario;
    use sui::clock;
    use slcl::enclave_registry::{Self, EnclaveRegistry, EnclaveAdminCap};

    fun fill(val: u8, len: u64): vector<u8> {
        let mut v = vector::empty<u8>();
        let mut i = 0u64;
        while (i < len) { vector::push_back(&mut v, val); i = i + 1; };
        v
    }

    #[test]
    fun test_update_expected_pcrs() {
        let admin = @0xA;
        let mut scenario = test_scenario::begin(admin);

        test_scenario::next_tx(&mut scenario, admin);
        { enclave_registry::init_for_testing(test_scenario::ctx(&mut scenario)); };

        test_scenario::next_tx(&mut scenario, admin);
        {
            let mut registry = test_scenario::take_shared<EnclaveRegistry>(&scenario);
            let cap = test_scenario::take_from_sender<EnclaveAdminCap>(&scenario);
            enclave_registry::update_expected_pcrs(
                &mut registry, &cap,
                fill(1, 48), fill(2, 48), fill(3, 48),
            );
            test_scenario::return_shared(registry);
            test_scenario::return_to_sender(&scenario, cap);
        };

        test_scenario::end(scenario);
    }

    #[test]
    fun test_register_enclave() {
        let admin = @0xA;
        let mut scenario = test_scenario::begin(admin);

        test_scenario::next_tx(&mut scenario, admin);
        { enclave_registry::init_for_testing(test_scenario::ctx(&mut scenario)); };

        // Set expected PCRs to empty so register_enclave passes PCR check
        test_scenario::next_tx(&mut scenario, admin);
        {
            let mut registry = test_scenario::take_shared<EnclaveRegistry>(&scenario);
            let cap = test_scenario::take_from_sender<EnclaveAdminCap>(&scenario);
            enclave_registry::update_expected_pcrs(
                &mut registry, &cap,
                vector::empty<u8>(), vector::empty<u8>(), vector::empty<u8>(),
            );
            test_scenario::return_shared(registry);
            test_scenario::return_to_sender(&scenario, cap);
        };

        // Register enclave — cap required after v0.5 admin gate
        test_scenario::next_tx(&mut scenario, admin);
        {
            let mut registry = test_scenario::take_shared<EnclaveRegistry>(&scenario);
            let cap = test_scenario::take_from_sender<EnclaveAdminCap>(&scenario);
            let clk = clock::create_for_testing(test_scenario::ctx(&mut scenario));

            enclave_registry::register_enclave(
                &mut registry,
                &cap,
                0,                   // validator_uid
                vector::empty<u8>(), // attestation_document (stubbed)
                fill(0, 32),         // enclave_pubkey (32 bytes)
                vector::empty<u8>(), // pcr0
                vector::empty<u8>(), // pcr1
                vector::empty<u8>(), // pcr2
                &clk,
                test_scenario::ctx(&mut scenario),
            );

            assert!(enclave_registry::is_registered(&registry, 0), 0);

            clock::destroy_for_testing(clk);
            test_scenario::return_shared(registry);
            test_scenario::return_to_sender(&scenario, cap);
        };

        test_scenario::end(scenario);
    }

    #[test]
    fun test_is_registered_false_for_unknown_uid() {
        let admin = @0xA;
        let mut scenario = test_scenario::begin(admin);

        test_scenario::next_tx(&mut scenario, admin);
        { enclave_registry::init_for_testing(test_scenario::ctx(&mut scenario)); };

        test_scenario::next_tx(&mut scenario, admin);
        {
            let registry = test_scenario::take_shared<EnclaveRegistry>(&scenario);
            assert!(!enclave_registry::is_registered(&registry, 999), 0);
            test_scenario::return_shared(registry);
        };

        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code = 2)] // E_PCR_MISMATCH
    fun test_register_enclave_pcr_mismatch() {
        let admin = @0xA;
        let mut scenario = test_scenario::begin(admin);

        test_scenario::next_tx(&mut scenario, admin);
        { enclave_registry::init_for_testing(test_scenario::ctx(&mut scenario)); };

        // Set expected PCRs to non-empty values
        test_scenario::next_tx(&mut scenario, admin);
        {
            let mut registry = test_scenario::take_shared<EnclaveRegistry>(&scenario);
            let cap = test_scenario::take_from_sender<EnclaveAdminCap>(&scenario);
            enclave_registry::update_expected_pcrs(
                &mut registry, &cap,
                fill(0xAA, 48), fill(0xBB, 48), fill(0xCC, 48),
            );
            test_scenario::return_shared(registry);
            test_scenario::return_to_sender(&scenario, cap);
        };

        // Attempt registration with wrong PCRs — must abort E_PCR_MISMATCH
        test_scenario::next_tx(&mut scenario, admin);
        {
            let mut registry = test_scenario::take_shared<EnclaveRegistry>(&scenario);
            let cap = test_scenario::take_from_sender<EnclaveAdminCap>(&scenario);
            let clk = clock::create_for_testing(test_scenario::ctx(&mut scenario));

            enclave_registry::register_enclave(
                &mut registry, &cap,
                0, vector::empty<u8>(), fill(0, 32),
                fill(0xFF, 48), // wrong pcr0
                fill(0xBB, 48),
                fill(0xCC, 48),
                &clk,
                test_scenario::ctx(&mut scenario),
            );

            clock::destroy_for_testing(clk);
            test_scenario::return_shared(registry);
            test_scenario::return_to_sender(&scenario, cap);
        };

        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code = 5)] // E_UNAUTHORIZED
    fun test_deactivate_enclave_wrong_owner() {
        let admin = @0xA;
        let attacker = @0xB;
        let mut scenario = test_scenario::begin(admin);

        test_scenario::next_tx(&mut scenario, admin);
        { enclave_registry::init_for_testing(test_scenario::ctx(&mut scenario)); };

        // Set empty PCRs
        test_scenario::next_tx(&mut scenario, admin);
        {
            let mut registry = test_scenario::take_shared<EnclaveRegistry>(&scenario);
            let cap = test_scenario::take_from_sender<EnclaveAdminCap>(&scenario);
            enclave_registry::update_expected_pcrs(
                &mut registry, &cap,
                vector::empty<u8>(), vector::empty<u8>(), vector::empty<u8>(),
            );
            test_scenario::return_shared(registry);
            test_scenario::return_to_sender(&scenario, cap);
        };

        // Register enclave as admin
        test_scenario::next_tx(&mut scenario, admin);
        {
            let mut registry = test_scenario::take_shared<EnclaveRegistry>(&scenario);
            let cap = test_scenario::take_from_sender<EnclaveAdminCap>(&scenario);
            let clk = clock::create_for_testing(test_scenario::ctx(&mut scenario));

            enclave_registry::register_enclave(
                &mut registry, &cap,
                0, vector::empty<u8>(), fill(0, 32),
                vector::empty<u8>(), vector::empty<u8>(), vector::empty<u8>(),
                &clk, test_scenario::ctx(&mut scenario),
            );

            clock::destroy_for_testing(clk);
            test_scenario::return_shared(registry);
            test_scenario::return_to_sender(&scenario, cap);
        };

        // Attacker tries to deactivate — must abort E_UNAUTHORIZED
        test_scenario::next_tx(&mut scenario, attacker);
        {
            let mut registry = test_scenario::take_shared<EnclaveRegistry>(&scenario);
            enclave_registry::deactivate_enclave(
                &mut registry, 0, test_scenario::ctx(&mut scenario),
            );
            test_scenario::return_shared(registry);
        };

        test_scenario::end(scenario);
    }
}
