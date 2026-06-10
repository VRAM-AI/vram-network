// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

#[test_only]
module slcl::validator_registry_tests {
    use sui::test_scenario;
    use sui::coin;
    use slcl::validator_registry::{Self, ValidatorRegistry, RegistryAdminCap, ValidatorTicket};

    // Dummy token type for testing
    public struct MOCK has drop {}

    // ── Helpers ────────────────────────────────────────────────────────────────

    fun setup(admin: address): test_scenario::Scenario {
        let mut s = test_scenario::begin(admin);
        test_scenario::next_tx(&mut s, admin);
        { validator_registry::init_for_testing(test_scenario::ctx(&mut s)); };
        test_scenario::next_tx(&mut s, admin);
        { validator_registry::create_registry_for_testing<MOCK>(1000, test_scenario::ctx(&mut s)); };
        s
    }

    // ── Bonding curve tests ────────────────────────────────────────────────────

    #[test]
    fun test_default_burn_tiers() {
        let admin = @0xA;
        let mut s = setup(admin);

        test_scenario::next_tx(&mut s, admin);
        {
            let reg = test_scenario::take_shared<ValidatorRegistry<MOCK>>(&s);
            let (t1, t2, t3, t4) = validator_registry::burn_tiers(&reg);
            assert!(t1 ==   2_100_000_000_000, 1);
            assert!(t2 ==   4_200_000_000_000, 2);
            assert!(t3 ==  10_500_000_000_000, 3);
            assert!(t4 ==  21_000_000_000_000, 4);
            // Slot 0 (first validator) → tier 1
            assert!(validator_registry::get_burn_amount(&reg) == t1, 5);
            test_scenario::return_shared(reg);
        };

        test_scenario::end(s);
    }

    #[test]
    fun test_update_burn_tiers_governance() {
        let admin = @0xA;
        let mut s = setup(admin);

        test_scenario::next_tx(&mut s, admin);
        {
            let mut reg = test_scenario::take_shared<ValidatorRegistry<MOCK>>(&s);
            let cap = test_scenario::take_from_sender<RegistryAdminCap>(&s);
            validator_registry::update_burn_tiers(&mut reg, &cap, 100, 200, 300, 400);
            let (t1, t2, t3, t4) = validator_registry::burn_tiers(&reg);
            assert!(t1 == 100, 1);
            assert!(t2 == 200, 2);
            assert!(t3 == 300, 3);
            assert!(t4 == 400, 4);
            // burn amount for next slot (count=0) should now be 100
            assert!(validator_registry::get_burn_amount(&reg) == 100, 5);
            test_scenario::return_shared(reg);
            test_scenario::return_to_sender(&s, cap);
        };

        test_scenario::end(s);
    }

    #[test]
    #[expected_failure(abort_code = 406)]
    fun test_update_burn_tiers_inverted_order_aborts() {
        let admin = @0xA;
        let mut s = setup(admin);

        test_scenario::next_tx(&mut s, admin);
        {
            let mut reg = test_scenario::take_shared<ValidatorRegistry<MOCK>>(&s);
            let cap = test_scenario::take_from_sender<RegistryAdminCap>(&s);
            // tier2 < tier1 — should abort 406
            validator_registry::update_burn_tiers(&mut reg, &cap, 200, 100, 300, 400);
            test_scenario::return_shared(reg);
            test_scenario::return_to_sender(&s, cap);
        };

        test_scenario::end(s);
    }

    #[test]
    #[expected_failure(abort_code = 406)]
    fun test_update_burn_tiers_zero_tier1_aborts() {
        let admin = @0xA;
        let mut s = setup(admin);

        test_scenario::next_tx(&mut s, admin);
        {
            let mut reg = test_scenario::take_shared<ValidatorRegistry<MOCK>>(&s);
            let cap = test_scenario::take_from_sender<RegistryAdminCap>(&s);
            validator_registry::update_burn_tiers(&mut reg, &cap, 0, 100, 200, 300);
            test_scenario::return_shared(reg);
            test_scenario::return_to_sender(&s, cap);
        };

        test_scenario::end(s);
    }

    // ── Registration tests ─────────────────────────────────────────────────────

    #[test]
    fun test_register_validator_with_burn() {
        let admin = @0xA;
        let miner = @0xB;
        let mut s = setup(admin);

        // First update to cheap burn amounts so we can mint test coins easily
        test_scenario::next_tx(&mut s, admin);
        {
            let mut reg = test_scenario::take_shared<ValidatorRegistry<MOCK>>(&s);
            let cap = test_scenario::take_from_sender<RegistryAdminCap>(&s);
            validator_registry::update_burn_tiers(&mut reg, &cap, 1000, 2000, 3000, 4000);
            test_scenario::return_shared(reg);
            test_scenario::return_to_sender(&s, cap);
        };

        test_scenario::next_tx(&mut s, miner);
        {
            let mut reg = test_scenario::take_shared<ValidatorRegistry<MOCK>>(&s);
            let burn_coin = coin::mint_for_testing<MOCK>(1000, test_scenario::ctx(&mut s));
            validator_registry::register_validator_with_burn(
                &mut reg, 42, 1000, burn_coin, 0,
                test_scenario::ctx(&mut s),
            );
            assert!(validator_registry::is_registered_validator(&reg, miner), 1);
            assert!(validator_registry::validator_count(&reg) == 1, 2);
            assert!(validator_registry::burn_vault_balance(&reg) == 1000, 3);
            test_scenario::return_shared(reg);
        };

        // Miner receives soulbound ticket
        test_scenario::next_tx(&mut s, miner);
        {
            let ticket = test_scenario::take_from_sender<ValidatorTicket>(&s);
            test_scenario::return_to_sender(&s, ticket);
        };

        test_scenario::end(s);
    }

    #[test]
    #[expected_failure(abort_code = 2)] // E_ALREADY_REGISTERED
    fun test_double_registration_aborts() {
        let admin = @0xA;
        let miner = @0xB;
        let mut s = setup(admin);

        test_scenario::next_tx(&mut s, admin);
        {
            let mut reg = test_scenario::take_shared<ValidatorRegistry<MOCK>>(&s);
            let cap = test_scenario::take_from_sender<RegistryAdminCap>(&s);
            validator_registry::update_burn_tiers(&mut reg, &cap, 100, 200, 300, 400);
            test_scenario::return_shared(reg);
            test_scenario::return_to_sender(&s, cap);
        };

        test_scenario::next_tx(&mut s, miner);
        {
            let mut reg = test_scenario::take_shared<ValidatorRegistry<MOCK>>(&s);
            let coin1 = coin::mint_for_testing<MOCK>(100, test_scenario::ctx(&mut s));
            validator_registry::register_validator_with_burn(
                &mut reg, 1, 1000, coin1, 0, test_scenario::ctx(&mut s),
            );
            test_scenario::return_shared(reg);
        };

        test_scenario::next_tx(&mut s, miner);
        {
            let mut reg = test_scenario::take_shared<ValidatorRegistry<MOCK>>(&s);
            let coin2 = coin::mint_for_testing<MOCK>(100, test_scenario::ctx(&mut s));
            // Second registration from same address — aborts
            validator_registry::register_validator_with_burn(
                &mut reg, 2, 1000, coin2, 0, test_scenario::ctx(&mut s),
            );
            test_scenario::return_shared(reg);
        };

        test_scenario::end(s);
    }

    #[test]
    #[expected_failure(abort_code = 405)] // E_INSUFFICIENT_BURN
    fun test_insufficient_burn_aborts() {
        let admin = @0xA;
        let miner = @0xB;
        let mut s = setup(admin);

        test_scenario::next_tx(&mut s, miner);
        {
            let mut reg = test_scenario::take_shared<ValidatorRegistry<MOCK>>(&s);
            // Default tier 1 burn = 2_100_000_000_000; send only 1
            let underpay = coin::mint_for_testing<MOCK>(1, test_scenario::ctx(&mut s));
            validator_registry::register_validator_with_burn(
                &mut reg, 1, 1000, underpay, 0, test_scenario::ctx(&mut s),
            );
            test_scenario::return_shared(reg);
        };

        test_scenario::end(s);
    }

    // ── Slash tests ────────────────────────────────────────────────────────────

    #[test]
    fun test_slash_reduces_stake_and_increments_count() {
        let admin = @0xA;
        let miner = @0xB;
        let mut s = setup(admin);

        // Register via test helper (no burn needed)
        test_scenario::next_tx(&mut s, miner);
        {
            let mut reg = test_scenario::take_shared<ValidatorRegistry<MOCK>>(&s);
            validator_registry::register_validator_for_testing(&mut reg, 7, 5000, test_scenario::ctx(&mut s));
            test_scenario::return_shared(reg);
        };

        test_scenario::next_tx(&mut s, admin);
        {
            let mut reg = test_scenario::take_shared<ValidatorRegistry<MOCK>>(&s);
            let cap = test_scenario::take_from_sender<RegistryAdminCap>(&s);
            validator_registry::slash_validator(&mut reg, &cap, miner, 1000);
            assert!(validator_registry::get_stake(&reg, miner) == 4000, 1);
            assert!(validator_registry::is_registered_validator(&reg, miner), 2);
            test_scenario::return_shared(reg);
            test_scenario::return_to_sender(&s, cap);
        };

        test_scenario::end(s);
    }

    #[test]
    fun test_slash_full_stake_deactivates() {
        let admin = @0xA;
        let miner = @0xB;
        let mut s = setup(admin);

        test_scenario::next_tx(&mut s, miner);
        {
            let mut reg = test_scenario::take_shared<ValidatorRegistry<MOCK>>(&s);
            validator_registry::register_validator_for_testing(&mut reg, 8, 1000, test_scenario::ctx(&mut s));
            test_scenario::return_shared(reg);
        };

        test_scenario::next_tx(&mut s, admin);
        {
            let mut reg = test_scenario::take_shared<ValidatorRegistry<MOCK>>(&s);
            let cap = test_scenario::take_from_sender<RegistryAdminCap>(&s);
            // Slash more than stake → deactivate
            validator_registry::slash_validator(&mut reg, &cap, miner, 9999);
            assert!(!validator_registry::is_registered_validator(&reg, miner), 1);
            test_scenario::return_shared(reg);
            test_scenario::return_to_sender(&s, cap);
        };

        test_scenario::end(s);
    }

    // ── Slot queries ───────────────────────────────────────────────────────────

    #[test]
    fun test_slots_remaining_decrements() {
        let admin = @0xA;
        let miner = @0xB;
        let mut s = setup(admin);

        test_scenario::next_tx(&mut s, miner);
        {
            let mut reg = test_scenario::take_shared<ValidatorRegistry<MOCK>>(&s);
            let before = validator_registry::slots_remaining(&reg);
            validator_registry::register_validator_for_testing(&mut reg, 1, 1000, test_scenario::ctx(&mut s));
            let after = validator_registry::slots_remaining(&reg);
            assert!(after == before - 1, 1);
            test_scenario::return_shared(reg);
        };

        test_scenario::end(s);
    }
}
