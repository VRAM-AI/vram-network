// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

#[test_only]
module slcl::hparams_tests {
    use sui::test_scenario;
    use slcl::hparams::{Self, Hparams, HparamsAdminCap};

    #[test]
    fun test_mainnet_emission_is_70_vram() {
        let admin = @0xA;
        let mut s = test_scenario::begin(admin);

        test_scenario::next_tx(&mut s, admin);
        { hparams::init_for_testing(test_scenario::ctx(&mut s)); };

        test_scenario::next_tx(&mut s, admin);
        {
            let h = test_scenario::take_shared<Hparams>(&s);
            assert!(hparams::emission_per_window(&h) == 70_000_000_000, 0);
            test_scenario::return_shared(h);
        };

        test_scenario::end(s);
    }

    #[test]
    fun test_mainnet_bps_sums_to_10000() {
        let admin = @0xA;
        let mut s = test_scenario::begin(admin);

        test_scenario::next_tx(&mut s, admin);
        { hparams::init_for_testing(test_scenario::ctx(&mut s)); };

        // Switch to mainnet BPS: 7143 miner / 2857 validator / 0 treasury
        // (treasury is 30% pre-minted at TGE — gets no per-window cut)
        test_scenario::next_tx(&mut s, admin);
        {
            let mut h  = test_scenario::take_shared<Hparams>(&s);
            let cap = test_scenario::take_from_sender<HparamsAdminCap>(&s);
            hparams::update_emission_split(&mut h, &cap, 7_143, 2_857, 0);
            assert!(hparams::miner_bps(&h) + hparams::validator_bps(&h) + hparams::treasury_bps(&h) == 10_000, 0);
            assert!(hparams::treasury_bps(&h) == 0, 1);
            test_scenario::return_shared(h);
            test_scenario::return_to_sender(&s, cap);
        };

        test_scenario::end(s);
    }

    #[test]
    fun test_halving_triggers_match_paper() {
        let admin = @0xA;
        let mut s = test_scenario::begin(admin);

        test_scenario::next_tx(&mut s, admin);
        { hparams::init_for_testing(test_scenario::ctx(&mut s)); };

        test_scenario::next_tx(&mut s, admin);
        {
            let h = test_scenario::take_shared<Hparams>(&s);
            assert!(hparams::halving_trigger_1(&h) ==  7_000_000_000_000_000, 0);
            assert!(hparams::halving_trigger_2(&h) == 10_500_000_000_000_000, 1);
            assert!(hparams::max_supply(&h)        == 21_000_000_000_000_000, 2);
            assert!(hparams::mining_allocation(&h) == 10_500_000_000_000_000, 3); // miners 50%
            test_scenario::return_shared(h);
        };

        test_scenario::end(s);
    }

    #[test]
    #[expected_failure(abort_code = 2)] // E_BPS_MUST_SUM_10000
    fun test_invalid_bps_split_aborts() {
        let admin = @0xA;
        let mut s = test_scenario::begin(admin);

        test_scenario::next_tx(&mut s, admin);
        { hparams::init_for_testing(test_scenario::ctx(&mut s)); };

        test_scenario::next_tx(&mut s, admin);
        {
            let mut h  = test_scenario::take_shared<Hparams>(&s);
            let cap = test_scenario::take_from_sender<HparamsAdminCap>(&s);
            // 5000 + 2000 + 2000 = 9000 ≠ 10000 → abort
            hparams::update_emission_split(&mut h, &cap, 5_000, 2_000, 2_000);
            test_scenario::return_shared(h);
            test_scenario::return_to_sender(&s, cap);
        };

        test_scenario::end(s);
    }
}
