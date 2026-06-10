// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

#[test_only]
module slcl::dataset_registry_tests {
    use sui::test_scenario;
    use slcl::dataset_registry::{Self, DatasetRegistry};

    #[test]
    fun test_register_increments_next_id() {
        let owner = @0xA;
        let mut s = test_scenario::begin(owner);

        test_scenario::next_tx(&mut s, owner);
        { dataset_registry::init_for_testing(test_scenario::ctx(&mut s)); };

        test_scenario::next_tx(&mut s, owner);
        {
            let mut reg: DatasetRegistry = test_scenario::take_shared(&s);
            assert!(dataset_registry::next_id(&reg) == 0, 0);

            dataset_registry::register_dataset(
                &mut reg, b"hash_a", 100, 5,
                test_scenario::ctx(&mut s),
            );
            assert!(dataset_registry::next_id(&reg) == 1, 1);

            dataset_registry::register_dataset(
                &mut reg, b"hash_b", 50, 10,
                test_scenario::ctx(&mut s),
            );
            assert!(dataset_registry::next_id(&reg) == 2, 2);

            test_scenario::return_shared(reg);
        };

        test_scenario::end(s);
    }

    #[test]
    fun test_shard_assignment_is_deterministic() {
        let owner = @0xA;
        let mut s = test_scenario::begin(owner);

        test_scenario::next_tx(&mut s, owner);
        { dataset_registry::init_for_testing(test_scenario::ctx(&mut s)); };

        test_scenario::next_tx(&mut s, owner);
        {
            let mut reg: DatasetRegistry = test_scenario::take_shared(&s);
            dataset_registry::register_dataset(
                &mut reg, b"manifest", 64, 5,
                test_scenario::ctx(&mut s),
            );

            let s1 = dataset_registry::assigned_shard(&reg, 0, 42, 100);
            let s2 = dataset_registry::assigned_shard(&reg, 0, 42, 100);
            assert!(s1 == s2, 0); // same inputs → same output

            test_scenario::return_shared(reg);
        };

        test_scenario::end(s);
    }

    #[test]
    fun test_shard_assignment_varies_by_window() {
        let owner = @0xA;
        let mut s = test_scenario::begin(owner);

        test_scenario::next_tx(&mut s, owner);
        { dataset_registry::init_for_testing(test_scenario::ctx(&mut s)); };

        test_scenario::next_tx(&mut s, owner);
        {
            let mut reg: DatasetRegistry = test_scenario::take_shared(&s);
            // Use large shard_count so XOR mixing has room to produce different results
            dataset_registry::register_dataset(
                &mut reg, b"manifest", 10_000, 5,
                test_scenario::ctx(&mut s),
            );

            let miner = 7u64;
            let mut seen_shard = dataset_registry::assigned_shard(&reg, 0, miner, 0);
            let mut all_same = true;
            let mut w = 1u64;
            while (w < 20) {
                let shard = dataset_registry::assigned_shard(&reg, 0, miner, w);
                if (shard != seen_shard) { all_same = false; };
                w = w + 1;
            };
            // At least one window should produce a different shard
            assert!(!all_same, 0);

            test_scenario::return_shared(reg);
        };

        test_scenario::end(s);
    }

    #[test]
    fun test_shard_in_range() {
        let owner = @0xA;
        let mut s = test_scenario::begin(owner);

        test_scenario::next_tx(&mut s, owner);
        { dataset_registry::init_for_testing(test_scenario::ctx(&mut s)); };

        test_scenario::next_tx(&mut s, owner);
        {
            let mut reg: DatasetRegistry = test_scenario::take_shared(&s);
            let shard_count = 7u64;
            dataset_registry::register_dataset(
                &mut reg, b"m", shard_count, 5,
                test_scenario::ctx(&mut s),
            );

            let mut i = 0u64;
            while (i < 50) {
                let shard = dataset_registry::assigned_shard(&reg, 0, i, i * 3);
                assert!(shard < shard_count, 0);
                i = i + 1;
            };

            test_scenario::return_shared(reg);
        };

        test_scenario::end(s);
    }

    #[test]
    fun test_revoke_deactivates_dataset() {
        let owner = @0xA;
        let mut s = test_scenario::begin(owner);

        test_scenario::next_tx(&mut s, owner);
        { dataset_registry::init_for_testing(test_scenario::ctx(&mut s)); };

        test_scenario::next_tx(&mut s, owner);
        {
            let mut reg: DatasetRegistry = test_scenario::take_shared(&s);
            dataset_registry::register_dataset(
                &mut reg, b"m", 10, 5,
                test_scenario::ctx(&mut s),
            );
            assert!(dataset_registry::is_active(&reg, 0), 0);

            dataset_registry::revoke_dataset(&mut reg, 0, test_scenario::ctx(&mut s));
            assert!(!dataset_registry::is_active(&reg, 0), 1);

            test_scenario::return_shared(reg);
        };

        test_scenario::end(s);
    }

    #[test]
    #[expected_failure(abort_code = 3)] // E_INACTIVE
    fun test_shard_on_inactive_dataset_aborts() {
        let owner = @0xA;
        let mut s = test_scenario::begin(owner);

        test_scenario::next_tx(&mut s, owner);
        { dataset_registry::init_for_testing(test_scenario::ctx(&mut s)); };

        test_scenario::next_tx(&mut s, owner);
        {
            let mut reg: DatasetRegistry = test_scenario::take_shared(&s);
            dataset_registry::register_dataset(
                &mut reg, b"m", 10, 5,
                test_scenario::ctx(&mut s),
            );
            dataset_registry::revoke_dataset(&mut reg, 0, test_scenario::ctx(&mut s));

            // This should abort with E_INACTIVE = 3
            dataset_registry::assigned_shard(&reg, 0, 1, 1);

            test_scenario::return_shared(reg);
        };

        test_scenario::end(s);
    }

    #[test]
    #[expected_failure(abort_code = 1)] // E_UNAUTHORIZED
    fun test_revoke_by_non_owner_aborts() {
        let owner   = @0xA;
        let attacker = @0xB;
        let mut s = test_scenario::begin(owner);

        test_scenario::next_tx(&mut s, owner);
        { dataset_registry::init_for_testing(test_scenario::ctx(&mut s)); };

        test_scenario::next_tx(&mut s, owner);
        {
            let mut reg: DatasetRegistry = test_scenario::take_shared(&s);
            dataset_registry::register_dataset(
                &mut reg, b"m", 10, 5,
                test_scenario::ctx(&mut s),
            );
            test_scenario::return_shared(reg);
        };

        // Attacker tries to revoke — must abort E_UNAUTHORIZED
        test_scenario::next_tx(&mut s, attacker);
        {
            let mut reg: DatasetRegistry = test_scenario::take_shared(&s);
            dataset_registry::revoke_dataset(&mut reg, 0, test_scenario::ctx(&mut s));
            test_scenario::return_shared(reg);
        };

        test_scenario::end(s);
    }
}
