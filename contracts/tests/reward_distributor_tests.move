// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

#[test_only]
module slcl::reward_distributor_tests {
    use sui::test_scenario::{Self, Scenario};
    use sui::coin::{Self, Coin};
    use sui::test_utils;
    use slcl::reward_distributor::{Self, RewardPool, DistributorAdminCap};
    use slcl::vram_token::{Self, VRAM_TOKEN};

    // ── Helpers ───────────────────────────────────────────────────────────────

    const ADMIN:    address = @0xAD;
    const MINER_A:  address = @0xA1;
    const MINER_B:  address = @0xA2;
    const TREASURY: address = @0xFE;

    /// Emission per window: 70 VRAM (mainnet Phase 1, in base units with 9 decimals)
    const EMISSION: u64 = 70_000_000_000;
    /// 1e9 = 1.0 in fixed-point weight scale
    const SCALE: u64 = 1_000_000_000;

    fun setup_pool(s: &mut Scenario) {
        test_scenario::next_tx(s, ADMIN);
        {
            let cap = test_scenario::take_from_sender<DistributorAdminCap>(s);
            reward_distributor::create_pool<VRAM_TOKEN>(
                &cap,
                EMISSION,
                TREASURY,
                0, // treasury_bps = 0 for simple tests
                test_scenario::ctx(s),
            );
            test_scenario::return_to_sender(s, cap);
        };
    }

    fun mint_to_pool(s: &mut Scenario, amount: u64) {
        test_scenario::next_tx(s, ADMIN);
        {
            let mut pool = test_scenario::take_shared<RewardPool<VRAM_TOKEN>>(s);
            let coin = coin::mint_for_testing<VRAM_TOKEN>(amount, test_scenario::ctx(s));
            reward_distributor::deposit<VRAM_TOKEN>(&mut pool, coin);
            test_scenario::return_shared(pool);
        };
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[test]
    fun test_single_winner_gets_full_emission() {
        let mut s = test_scenario::begin(ADMIN);

        // Init token (creates TreasuryCap + DistributorAdminCap via module init)
        test_scenario::next_tx(&mut s, ADMIN);
        { vram_token::init_for_testing(test_scenario::ctx(&mut s)); };

        test_scenario::next_tx(&mut s, ADMIN);
        { reward_distributor::init_for_testing(test_scenario::ctx(&mut s)); };

        setup_pool(&mut s);
        mint_to_pool(&mut s, EMISSION * 10); // fund pool with 10 windows of rewards

        // Window 1: single miner with full weight (1e9)
        test_scenario::next_tx(&mut s, ADMIN);
        {
            let mut pool = test_scenario::take_shared<RewardPool<VRAM_TOKEN>>(&s);
            reward_distributor::distribute_rewards<VRAM_TOKEN>(
                &mut pool,
                1, // window
                vector[1u64],              // peer_uids
                vector[SCALE],             // normalized_weights: 1.0 = full emission
                vector[MINER_A],           // peer_addresses
                test_scenario::ctx(&mut s),
            );
            test_scenario::return_shared(pool);
        };

        // Miner A should have received EMISSION coins
        test_scenario::next_tx(&mut s, MINER_A);
        {
            let coin = test_scenario::take_from_sender<Coin<VRAM_TOKEN>>(&s);
            assert!(coin::value(&coin) == EMISSION, 0);
            test_utils::destroy(coin);
        };

        test_scenario::end(s);
    }

    #[test]
    fun test_two_miners_split_proportional() {
        let mut s = test_scenario::begin(ADMIN);

        test_scenario::next_tx(&mut s, ADMIN);
        { vram_token::init_for_testing(test_scenario::ctx(&mut s)); };
        test_scenario::next_tx(&mut s, ADMIN);
        { reward_distributor::init_for_testing(test_scenario::ctx(&mut s)); };

        setup_pool(&mut s);
        mint_to_pool(&mut s, EMISSION * 10);

        // Miner A: 75%, Miner B: 25% (weights sum to 1e9)
        let weight_a: u64 = 750_000_000; // 0.75
        let weight_b: u64 = 250_000_000; // 0.25

        test_scenario::next_tx(&mut s, ADMIN);
        {
            let mut pool = test_scenario::take_shared<RewardPool<VRAM_TOKEN>>(&s);
            reward_distributor::distribute_rewards<VRAM_TOKEN>(
                &mut pool,
                1,
                vector[1u64, 2u64],
                vector[weight_a, weight_b],
                vector[MINER_A, MINER_B],
                test_scenario::ctx(&mut s),
            );
            test_scenario::return_shared(pool);
        };

        test_scenario::next_tx(&mut s, MINER_A);
        {
            let coin = test_scenario::take_from_sender<Coin<VRAM_TOKEN>>(&s);
            let expected = (EMISSION as u128) * (weight_a as u128) / (SCALE as u128);
            assert!(coin::value(&coin) == (expected as u64), 1);
            test_utils::destroy(coin);
        };

        test_scenario::next_tx(&mut s, MINER_B);
        {
            let coin = test_scenario::take_from_sender<Coin<VRAM_TOKEN>>(&s);
            let expected = (EMISSION as u128) * (weight_b as u128) / (SCALE as u128);
            assert!(coin::value(&coin) == (expected as u64), 2);
            test_utils::destroy(coin);
        };

        test_scenario::end(s);
    }

    #[test]
    fun test_treasury_cut_is_enforced() {
        let mut s = test_scenario::begin(ADMIN);

        test_scenario::next_tx(&mut s, ADMIN);
        { vram_token::init_for_testing(test_scenario::ctx(&mut s)); };
        test_scenario::next_tx(&mut s, ADMIN);
        { reward_distributor::init_for_testing(test_scenario::ctx(&mut s)); };

        // Create pool with 30% treasury cut (3000 bps = mainnet config)
        test_scenario::next_tx(&mut s, ADMIN);
        {
            let cap = test_scenario::take_from_sender<DistributorAdminCap>(&s);
            reward_distributor::create_pool<VRAM_TOKEN>(
                &cap,
                EMISSION,
                TREASURY,
                3000, // 30% treasury
                test_scenario::ctx(&mut s),
            );
            test_scenario::return_to_sender(&s, cap);
        };

        mint_to_pool(&mut s, EMISSION * 10);

        // Single miner gets 100% of miner allocation (after treasury cut)
        test_scenario::next_tx(&mut s, ADMIN);
        {
            let mut pool = test_scenario::take_shared<RewardPool<VRAM_TOKEN>>(&s);
            reward_distributor::distribute_rewards<VRAM_TOKEN>(
                &mut pool,
                1,
                vector[1u64],
                vector[SCALE],
                vector[MINER_A],
                test_scenario::ctx(&mut s),
            );
            test_scenario::return_shared(pool);
        };

        // Treasury gets 30% of emission
        test_scenario::next_tx(&mut s, TREASURY);
        {
            let coin = test_scenario::take_from_sender<Coin<VRAM_TOKEN>>(&s);
            let expected_treasury = EMISSION * 3000 / 10000;
            assert!(coin::value(&coin) == expected_treasury, 3);
            test_utils::destroy(coin);
        };

        // Miner gets 70% of emission
        test_scenario::next_tx(&mut s, MINER_A);
        {
            let coin = test_scenario::take_from_sender<Coin<VRAM_TOKEN>>(&s);
            let expected_miner = EMISSION - (EMISSION * 3000 / 10000);
            assert!(coin::value(&coin) == expected_miner, 4);
            test_utils::destroy(coin);
        };

        test_scenario::end(s);
    }

    #[test]
    fun test_no_double_distribution_same_window() {
        let mut s = test_scenario::begin(ADMIN);

        test_scenario::next_tx(&mut s, ADMIN);
        { vram_token::init_for_testing(test_scenario::ctx(&mut s)); };
        test_scenario::next_tx(&mut s, ADMIN);
        { reward_distributor::init_for_testing(test_scenario::ctx(&mut s)); };

        setup_pool(&mut s);
        mint_to_pool(&mut s, EMISSION * 10);

        // First distribution — window 1
        test_scenario::next_tx(&mut s, ADMIN);
        {
            let mut pool = test_scenario::take_shared<RewardPool<VRAM_TOKEN>>(&s);
            reward_distributor::distribute_rewards<VRAM_TOKEN>(
                &mut pool, 1, vector[1u64], vector[SCALE], vector[MINER_A],
                test_scenario::ctx(&mut s),
            );
            test_scenario::return_shared(pool);
        };

        test_scenario::end(s);
    }

    #[test]
    #[expected_failure(abort_code = slcl::reward_distributor::E_REWARDS_ALREADY_DISTRIBUTED)]
    fun test_double_distribution_aborts() {
        let mut s = test_scenario::begin(ADMIN);

        test_scenario::next_tx(&mut s, ADMIN);
        { vram_token::init_for_testing(test_scenario::ctx(&mut s)); };
        test_scenario::next_tx(&mut s, ADMIN);
        { reward_distributor::init_for_testing(test_scenario::ctx(&mut s)); };

        setup_pool(&mut s);
        mint_to_pool(&mut s, EMISSION * 10);

        // First distribution
        test_scenario::next_tx(&mut s, ADMIN);
        {
            let mut pool = test_scenario::take_shared<RewardPool<VRAM_TOKEN>>(&s);
            reward_distributor::distribute_rewards<VRAM_TOKEN>(
                &mut pool, 1, vector[1u64], vector[SCALE], vector[MINER_A],
                test_scenario::ctx(&mut s),
            );
            test_scenario::return_shared(pool);
        };

        // Second distribution same window — must abort
        test_scenario::next_tx(&mut s, ADMIN);
        {
            let mut pool = test_scenario::take_shared<RewardPool<VRAM_TOKEN>>(&s);
            reward_distributor::distribute_rewards<VRAM_TOKEN>(
                &mut pool, 1, vector[1u64], vector[SCALE], vector[MINER_A],
                test_scenario::ctx(&mut s),
            );
            test_scenario::return_shared(pool);
        };

        test_scenario::end(s);
    }

    #[test]
    fun test_multiple_windows_sequential() {
        let mut s = test_scenario::begin(ADMIN);

        test_scenario::next_tx(&mut s, ADMIN);
        { vram_token::init_for_testing(test_scenario::ctx(&mut s)); };
        test_scenario::next_tx(&mut s, ADMIN);
        { reward_distributor::init_for_testing(test_scenario::ctx(&mut s)); };

        setup_pool(&mut s);
        mint_to_pool(&mut s, EMISSION * 10);

        // Windows 1, 2, 3 — each pays out correctly
        let mut w: u64 = 1;
        while (w <= 3) {
            test_scenario::next_tx(&mut s, ADMIN);
            {
                let mut pool = test_scenario::take_shared<RewardPool<VRAM_TOKEN>>(&s);
                reward_distributor::distribute_rewards<VRAM_TOKEN>(
                    &mut pool, w, vector[1u64], vector[SCALE], vector[MINER_A],
                    test_scenario::ctx(&mut s),
                );
                test_scenario::return_shared(pool);
            };
            w = w + 1;
        };

        // Miner A received 3 × EMISSION coins (one per window)
        test_scenario::next_tx(&mut s, MINER_A);
        {
            // Each window sends a separate Coin object
            let c1 = test_scenario::take_from_sender<Coin<VRAM_TOKEN>>(&s);
            let c2 = test_scenario::take_from_sender<Coin<VRAM_TOKEN>>(&s);
            let c3 = test_scenario::take_from_sender<Coin<VRAM_TOKEN>>(&s);
            assert!(coin::value(&c1) + coin::value(&c2) + coin::value(&c3) == EMISSION * 3, 5);
            test_utils::destroy(c1);
            test_utils::destroy(c2);
            test_utils::destroy(c3);
        };

        test_scenario::end(s);
    }

    // ── Cumulative-earned (TGE airdrop snapshot) ──────────────────────────────

    #[test]
    fun test_cumulative_unknown_address_returns_zero() {
        let mut s = test_scenario::begin(ADMIN);

        test_scenario::next_tx(&mut s, ADMIN);
        { vram_token::init_for_testing(test_scenario::ctx(&mut s)); };
        test_scenario::next_tx(&mut s, ADMIN);
        { reward_distributor::init_for_testing(test_scenario::ctx(&mut s)); };

        setup_pool(&mut s);

        test_scenario::next_tx(&mut s, ADMIN);
        {
            let pool = test_scenario::take_shared<RewardPool<VRAM_TOKEN>>(&s);
            assert!(reward_distributor::cumulative_earned_of<VRAM_TOKEN>(&pool, MINER_A) == 0, 100);
            assert!(!reward_distributor::has_earned<VRAM_TOKEN>(&pool, MINER_A), 101);
            test_scenario::return_shared(pool);
        };

        test_scenario::end(s);
    }

    #[test]
    fun test_cumulative_single_window_records_payout() {
        let mut s = test_scenario::begin(ADMIN);

        test_scenario::next_tx(&mut s, ADMIN);
        { vram_token::init_for_testing(test_scenario::ctx(&mut s)); };
        test_scenario::next_tx(&mut s, ADMIN);
        { reward_distributor::init_for_testing(test_scenario::ctx(&mut s)); };

        setup_pool(&mut s);
        mint_to_pool(&mut s, EMISSION * 10);

        // 75/25 split — same weights as test_two_miners_split_proportional
        let weight_a: u64 = 750_000_000;
        let weight_b: u64 = 250_000_000;

        test_scenario::next_tx(&mut s, ADMIN);
        {
            let mut pool = test_scenario::take_shared<RewardPool<VRAM_TOKEN>>(&s);
            reward_distributor::distribute_rewards<VRAM_TOKEN>(
                &mut pool,
                1,
                vector[1u64, 2u64],
                vector[weight_a, weight_b],
                vector[MINER_A, MINER_B],
                test_scenario::ctx(&mut s),
            );
            test_scenario::return_shared(pool);
        };

        test_scenario::next_tx(&mut s, ADMIN);
        {
            let pool = test_scenario::take_shared<RewardPool<VRAM_TOKEN>>(&s);
            let expected_a = (EMISSION as u128) * (weight_a as u128) / (SCALE as u128);
            let expected_b = (EMISSION as u128) * (weight_b as u128) / (SCALE as u128);
            assert!(reward_distributor::cumulative_earned_of<VRAM_TOKEN>(&pool, MINER_A) == (expected_a as u64), 102);
            assert!(reward_distributor::cumulative_earned_of<VRAM_TOKEN>(&pool, MINER_B) == (expected_b as u64), 103);
            assert!(reward_distributor::has_earned<VRAM_TOKEN>(&pool, MINER_A), 104);
            assert!(reward_distributor::has_earned<VRAM_TOKEN>(&pool, MINER_B), 105);
            test_scenario::return_shared(pool);
        };

        test_scenario::end(s);
    }

    #[test]
    fun test_cumulative_accumulates_across_windows() {
        let mut s = test_scenario::begin(ADMIN);

        test_scenario::next_tx(&mut s, ADMIN);
        { vram_token::init_for_testing(test_scenario::ctx(&mut s)); };
        test_scenario::next_tx(&mut s, ADMIN);
        { reward_distributor::init_for_testing(test_scenario::ctx(&mut s)); };

        setup_pool(&mut s);
        mint_to_pool(&mut s, EMISSION * 10);

        // Three identical windows, single miner = 1.0 weight each time.
        let mut w: u64 = 1;
        while (w <= 3) {
            test_scenario::next_tx(&mut s, ADMIN);
            {
                let mut pool = test_scenario::take_shared<RewardPool<VRAM_TOKEN>>(&s);
                reward_distributor::distribute_rewards<VRAM_TOKEN>(
                    &mut pool, w, vector[1u64], vector[SCALE], vector[MINER_A],
                    test_scenario::ctx(&mut s),
                );
                test_scenario::return_shared(pool);
            };
            w = w + 1;
        };

        test_scenario::next_tx(&mut s, ADMIN);
        {
            let pool = test_scenario::take_shared<RewardPool<VRAM_TOKEN>>(&s);
            assert!(reward_distributor::cumulative_earned_of<VRAM_TOKEN>(&pool, MINER_A) == EMISSION * 3, 106);
            test_scenario::return_shared(pool);
        };

        test_scenario::end(s);
    }

    #[test]
    fun test_cumulative_excludes_treasury_cut() {
        // Treasury transfers are NOT tracked in cumulative_earned —
        // only peer payouts. The airdrop snapshot reads peers only.
        let mut s = test_scenario::begin(ADMIN);

        test_scenario::next_tx(&mut s, ADMIN);
        { vram_token::init_for_testing(test_scenario::ctx(&mut s)); };
        test_scenario::next_tx(&mut s, ADMIN);
        { reward_distributor::init_for_testing(test_scenario::ctx(&mut s)); };

        // 30% treasury bps (mainnet config)
        test_scenario::next_tx(&mut s, ADMIN);
        {
            let cap = test_scenario::take_from_sender<DistributorAdminCap>(&s);
            reward_distributor::create_pool<VRAM_TOKEN>(
                &cap, EMISSION, TREASURY, 3000, test_scenario::ctx(&mut s),
            );
            test_scenario::return_to_sender(&s, cap);
        };

        mint_to_pool(&mut s, EMISSION * 10);

        test_scenario::next_tx(&mut s, ADMIN);
        {
            let mut pool = test_scenario::take_shared<RewardPool<VRAM_TOKEN>>(&s);
            reward_distributor::distribute_rewards<VRAM_TOKEN>(
                &mut pool, 1, vector[1u64], vector[SCALE], vector[MINER_A],
                test_scenario::ctx(&mut s),
            );
            test_scenario::return_shared(pool);
        };

        test_scenario::next_tx(&mut s, ADMIN);
        {
            let pool = test_scenario::take_shared<RewardPool<VRAM_TOKEN>>(&s);
            let expected_miner = EMISSION - (EMISSION * 3000 / 10000);
            assert!(reward_distributor::cumulative_earned_of<VRAM_TOKEN>(&pool, MINER_A) == expected_miner, 107);
            // Treasury receives via direct transfer, not the peer loop — must be 0.
            assert!(reward_distributor::cumulative_earned_of<VRAM_TOKEN>(&pool, TREASURY) == 0, 108);
            assert!(!reward_distributor::has_earned<VRAM_TOKEN>(&pool, TREASURY), 109);
            test_scenario::return_shared(pool);
        };

        test_scenario::end(s);
    }

    #[test]
    #[expected_failure(abort_code = slcl::reward_distributor::E_EMPTY_REWARD_POOL)]
    fun test_empty_pool_aborts() {
        let mut s = test_scenario::begin(ADMIN);

        test_scenario::next_tx(&mut s, ADMIN);
        { vram_token::init_for_testing(test_scenario::ctx(&mut s)); };
        test_scenario::next_tx(&mut s, ADMIN);
        { reward_distributor::init_for_testing(test_scenario::ctx(&mut s)); };

        setup_pool(&mut s);
        // No deposit — pool is empty

        test_scenario::next_tx(&mut s, ADMIN);
        {
            let mut pool = test_scenario::take_shared<RewardPool<VRAM_TOKEN>>(&s);
            reward_distributor::distribute_rewards<VRAM_TOKEN>(
                &mut pool, 1, vector[1u64], vector[SCALE], vector[MINER_A],
                test_scenario::ctx(&mut s),
            );
            test_scenario::return_shared(pool);
        };

        test_scenario::end(s);
    }
}
