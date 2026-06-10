// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

#[test_only]
module slcl::staking_pool_tests {
    use sui::test_scenario;
    use sui::coin::{Self, Coin};
    use sui::clock;
    use slcl::staking_pool::{Self, StakingPool, StakerTicket};

    /// Test-only phantom token type.
    public struct TOK has drop {}

    // ── Helper ─────────────────────────────────────────────────────────────────

    fun tok(amount: u64, ctx: &mut sui::tx_context::TxContext): Coin<TOK> {
        coin::mint_for_testing<TOK>(amount, ctx)
    }

    // ── Tests ──────────────────────────────────────────────────────────────────

    #[test]
    fun test_stake_and_claim_full_reward() {
        let user = @0xA;
        let mut s = test_scenario::begin(user);

        test_scenario::next_tx(&mut s, user);
        { staking_pool::create_pool<TOK>(test_scenario::ctx(&mut s)); };

        // Stake 1_000_000_000_000 (1000 VRAM) at tier 0 (1.0×)
        test_scenario::next_tx(&mut s, user);
        {
            let mut pool: StakingPool<TOK> = test_scenario::take_shared(&s);
            let clk = clock::create_for_testing(test_scenario::ctx(&mut s));
            staking_pool::stake(
                &mut pool,
                tok(1_000_000_000_000, test_scenario::ctx(&mut s)),
                0, &clk, test_scenario::ctx(&mut s),
            );
            assert!(staking_pool::pool_stake_balance(&pool) == 1_000_000_000_000, 0);
            clock::destroy_for_testing(clk);
            test_scenario::return_shared(pool);
        };

        // Deposit 100_000_000_000 rewards (100 VRAM)
        test_scenario::next_tx(&mut s, user);
        {
            let mut pool: StakingPool<TOK> = test_scenario::take_shared(&s);
            staking_pool::deposit_rewards(&mut pool, tok(100_000_000_000, test_scenario::ctx(&mut s)));
            assert!(staking_pool::pool_reward_balance(&pool) == 100_000_000_000, 0);
            test_scenario::return_shared(pool);
        };

        // Claim — sole staker gets 100% of rewards
        test_scenario::next_tx(&mut s, user);
        {
            let mut pool: StakingPool<TOK>         = test_scenario::take_shared(&s);
            let mut ticket: StakerTicket<TOK> = test_scenario::take_from_sender(&s);
            staking_pool::claim_rewards(&mut pool, &mut ticket, test_scenario::ctx(&mut s));
            assert!(staking_pool::pool_reward_balance(&pool) == 0, 0);
            test_scenario::return_shared(pool);
            test_scenario::return_to_sender(&s, ticket);
        };

        // Verify claimed coin arrived
        test_scenario::next_tx(&mut s, user);
        {
            let coin: Coin<TOK> = test_scenario::take_from_sender(&s);
            assert!(coin::value(&coin) == 100_000_000_000, 0);
            test_scenario::return_to_sender(&s, coin);
        };

        test_scenario::end(s);
    }

    #[test]
    fun test_two_stakers_proportional_rewards() {
        // A: 1000 VRAM tier 0 (1.0×) → weight = 1_000_000_000_000
        // B: 1000 VRAM tier 1 (1.2×) → weight = 1_200_000_000_000
        // Deposit 2200 VRAM → A:1000, B:1200
        let a = @0xA;
        let b = @0xB;
        let mut s = test_scenario::begin(a);

        test_scenario::next_tx(&mut s, a);
        { staking_pool::create_pool<TOK>(test_scenario::ctx(&mut s)); };

        test_scenario::next_tx(&mut s, a);
        {
            let mut pool: StakingPool<TOK> = test_scenario::take_shared(&s);
            let clk = clock::create_for_testing(test_scenario::ctx(&mut s));
            staking_pool::stake(
                &mut pool, tok(1_000_000_000_000, test_scenario::ctx(&mut s)),
                0, &clk, test_scenario::ctx(&mut s),
            );
            clock::destroy_for_testing(clk);
            test_scenario::return_shared(pool);
        };

        test_scenario::next_tx(&mut s, b);
        {
            let mut pool: StakingPool<TOK> = test_scenario::take_shared(&s);
            let clk = clock::create_for_testing(test_scenario::ctx(&mut s));
            staking_pool::stake(
                &mut pool, tok(1_000_000_000_000, test_scenario::ctx(&mut s)),
                1, &clk, test_scenario::ctx(&mut s),
            );
            clock::destroy_for_testing(clk);
            test_scenario::return_shared(pool);
        };

        // Deposit rewards worth (A_weight + B_weight) so each gets exactly their weight
        test_scenario::next_tx(&mut s, a);
        {
            let mut pool: StakingPool<TOK> = test_scenario::take_shared(&s);
            staking_pool::deposit_rewards(&mut pool, tok(2_200_000_000_000, test_scenario::ctx(&mut s)));
            test_scenario::return_shared(pool);
        };

        // A claims
        test_scenario::next_tx(&mut s, a);
        {
            let mut pool: StakingPool<TOK>         = test_scenario::take_shared(&s);
            let mut ticket: StakerTicket<TOK> = test_scenario::take_from_sender(&s);
            staking_pool::claim_rewards(&mut pool, &mut ticket, test_scenario::ctx(&mut s));
            test_scenario::return_shared(pool);
            test_scenario::return_to_sender(&s, ticket);
        };
        test_scenario::next_tx(&mut s, a);
        {
            let coin: Coin<TOK> = test_scenario::take_from_sender(&s);
            assert!(coin::value(&coin) == 1_000_000_000_000, 0);
            test_scenario::return_to_sender(&s, coin);
        };

        // B claims
        test_scenario::next_tx(&mut s, b);
        {
            let mut pool: StakingPool<TOK>         = test_scenario::take_shared(&s);
            let mut ticket: StakerTicket<TOK> = test_scenario::take_from_sender(&s);
            staking_pool::claim_rewards(&mut pool, &mut ticket, test_scenario::ctx(&mut s));
            test_scenario::return_shared(pool);
            test_scenario::return_to_sender(&s, ticket);
        };
        test_scenario::next_tx(&mut s, b);
        {
            let coin: Coin<TOK> = test_scenario::take_from_sender(&s);
            assert!(coin::value(&coin) == 1_200_000_000_000, 0);
            test_scenario::return_to_sender(&s, coin);
        };

        test_scenario::end(s);
    }

    #[test]
    fun test_early_unstake_25pct_penalty() {
        let user = @0xA;
        let mut s = test_scenario::begin(user);

        test_scenario::next_tx(&mut s, user);
        { staking_pool::create_pool<TOK>(test_scenario::ctx(&mut s)); };

        // Stake 1000 at t=0 with 30d lockup
        test_scenario::next_tx(&mut s, user);
        {
            let mut pool: StakingPool<TOK> = test_scenario::take_shared(&s);
            let clk = clock::create_for_testing(test_scenario::ctx(&mut s));
            staking_pool::stake(
                &mut pool, tok(1_000_000_000_000, test_scenario::ctx(&mut s)),
                0, &clk, test_scenario::ctx(&mut s),
            );
            clock::destroy_for_testing(clk);
            test_scenario::return_shared(pool);
        };

        // Unstake immediately (clock still at 0ms → lockup not expired)
        test_scenario::next_tx(&mut s, user);
        {
            let mut pool: StakingPool<TOK>    = test_scenario::take_shared(&s);
            let ticket: StakerTicket<TOK> = test_scenario::take_from_sender(&s);
            let clk = clock::create_for_testing(test_scenario::ctx(&mut s));
            staking_pool::unstake(&mut pool, ticket, &clk, test_scenario::ctx(&mut s));
            clock::destroy_for_testing(clk);
            test_scenario::return_shared(pool);
        };

        // Should receive 750 VRAM (1000 − 25% = 750)
        test_scenario::next_tx(&mut s, user);
        {
            let coin: Coin<TOK> = test_scenario::take_from_sender(&s);
            assert!(coin::value(&coin) == 750_000_000_000, 0);
            test_scenario::return_to_sender(&s, coin);
        };

        test_scenario::end(s);
    }

    #[test]
    fun test_unstake_after_lockup_no_penalty() {
        let user = @0xA;
        let mut s = test_scenario::begin(user);

        test_scenario::next_tx(&mut s, user);
        { staking_pool::create_pool<TOK>(test_scenario::ctx(&mut s)); };

        test_scenario::next_tx(&mut s, user);
        {
            let mut pool: StakingPool<TOK> = test_scenario::take_shared(&s);
            let clk = clock::create_for_testing(test_scenario::ctx(&mut s));
            staking_pool::stake(
                &mut pool, tok(1_000_000_000_000, test_scenario::ctx(&mut s)),
                0, &clk, test_scenario::ctx(&mut s),
            );
            clock::destroy_for_testing(clk);
            test_scenario::return_shared(pool);
        };

        // Unstake after lockup (30d + 1ms = 2_592_000_001)
        test_scenario::next_tx(&mut s, user);
        {
            let mut pool: StakingPool<TOK>    = test_scenario::take_shared(&s);
            let ticket: StakerTicket<TOK> = test_scenario::take_from_sender(&s);
            let mut clk = clock::create_for_testing(test_scenario::ctx(&mut s));
            clock::set_for_testing(&mut clk, 2_592_000_001);
            staking_pool::unstake(&mut pool, ticket, &clk, test_scenario::ctx(&mut s));
            clock::destroy_for_testing(clk);
            test_scenario::return_shared(pool);
        };

        test_scenario::next_tx(&mut s, user);
        {
            let coin: Coin<TOK> = test_scenario::take_from_sender(&s);
            assert!(coin::value(&coin) == 1_000_000_000_000, 0); // full principal
            test_scenario::return_to_sender(&s, coin);
        };

        test_scenario::end(s);
    }

    #[test]
    fun test_deposit_no_stakers_is_safe() {
        let admin = @0xA;
        let mut s = test_scenario::begin(admin);

        test_scenario::next_tx(&mut s, admin);
        { staking_pool::create_pool<TOK>(test_scenario::ctx(&mut s)); };

        test_scenario::next_tx(&mut s, admin);
        {
            let mut pool: StakingPool<TOK> = test_scenario::take_shared(&s);
            staking_pool::deposit_rewards(&mut pool, tok(500_000_000_000, test_scenario::ctx(&mut s)));
            assert!(staking_pool::pool_reward_balance(&pool) == 500_000_000_000, 0);
            assert!(staking_pool::total_stake_weight(&pool)  == 0, 1);
            test_scenario::return_shared(pool);
        };

        test_scenario::end(s);
    }

    #[test]
    #[expected_failure(abort_code = 1)] // E_NOTHING_TO_CLAIM
    fun test_claim_without_rewards_aborts() {
        let user = @0xA;
        let mut s = test_scenario::begin(user);

        test_scenario::next_tx(&mut s, user);
        { staking_pool::create_pool<TOK>(test_scenario::ctx(&mut s)); };

        test_scenario::next_tx(&mut s, user);
        {
            let mut pool: StakingPool<TOK> = test_scenario::take_shared(&s);
            let clk = clock::create_for_testing(test_scenario::ctx(&mut s));
            staking_pool::stake(
                &mut pool, tok(1_000_000_000_000, test_scenario::ctx(&mut s)),
                0, &clk, test_scenario::ctx(&mut s),
            );
            clock::destroy_for_testing(clk);
            test_scenario::return_shared(pool);
        };

        test_scenario::next_tx(&mut s, user);
        {
            let mut pool: StakingPool<TOK>         = test_scenario::take_shared(&s);
            let mut ticket: StakerTicket<TOK> = test_scenario::take_from_sender(&s);
            staking_pool::claim_rewards(&mut pool, &mut ticket, test_scenario::ctx(&mut s));
            test_scenario::return_shared(pool);
            test_scenario::return_to_sender(&s, ticket);
        };

        test_scenario::end(s);
    }

    #[test]
    #[expected_failure(abort_code = 2)] // E_INVALID_TIER
    fun test_invalid_lockup_tier_aborts() {
        let user = @0xA;
        let mut s = test_scenario::begin(user);

        test_scenario::next_tx(&mut s, user);
        { staking_pool::create_pool<TOK>(test_scenario::ctx(&mut s)); };

        test_scenario::next_tx(&mut s, user);
        {
            let mut pool: StakingPool<TOK> = test_scenario::take_shared(&s);
            let clk = clock::create_for_testing(test_scenario::ctx(&mut s));
            staking_pool::stake(
                &mut pool, tok(1_000_000_000_000, test_scenario::ctx(&mut s)),
                4, &clk, test_scenario::ctx(&mut s), // tier 4 does not exist
            );
            clock::destroy_for_testing(clk);
            test_scenario::return_shared(pool);
        };

        test_scenario::end(s);
    }
}
