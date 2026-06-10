// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

#[test_only]
module slcl::tokenomics_tests {
    use slcl::reward_distributor;
    use slcl::vram_token;

    // ── Supply sanity ──────────────────────────────────────────────────────────

    #[test]
    fun test_total_supply_adds_to_21m() {
        // Mining (50%) + Treasury (30%) + Team (8%) + Liquidity (7%) + Airdrop (5%)
        let total =
            vram_token::mining_allocation()    +  // 10.5M
            vram_token::treasury_allocation()  +  //  6.3M
            vram_token::team_allocation()      +  //  1.68M
            vram_token::liquidity_allocation() +  //  1.47M
            vram_token::airdrop_allocation();     //  1.05M
        assert!(total == 21_000_000_000_000_000, 0);
    }

    #[test]
    fun test_emission_is_50_pct_of_cap() {
        // mining_allocation (10.5M) = 50% of 21M hard cap
        let total =
            vram_token::mining_allocation()    +
            vram_token::treasury_allocation()  +
            vram_token::team_allocation()      +
            vram_token::liquidity_allocation() +
            vram_token::airdrop_allocation();
        assert!(vram_token::mining_allocation() * 2 == total, 0);
    }

    #[test]
    fun test_premint_is_50_pct_of_cap() {
        // Pre-minted at TGE: treasury + team + liquidity + airdrop = 10.5M (50%)
        let premint =
            vram_token::treasury_allocation()  +
            vram_token::team_allocation()      +
            vram_token::liquidity_allocation() +
            vram_token::airdrop_allocation();
        assert!(premint == 10_500_000_000_000_000, 0);
    }

    // ── Halving logic ──────────────────────────────────────────────────────────

    #[test]
    fun test_emission_genesis_rate() {
        // 0 tokens issued → Phase 1: 70 VRAM/window
        let rate = reward_distributor::current_emission_rate(70_000_000_000, 0);
        assert!(rate == 70_000_000_000, 0);
    }

    #[test]
    fun test_emission_just_before_first_halving() {
        let rate = reward_distributor::current_emission_rate(
            70_000_000_000,
            6_999_999_999_999_999, // 1 raw unit before 7M threshold
        );
        assert!(rate == 70_000_000_000, 0);
    }

    #[test]
    fun test_emission_at_first_halving() {
        // exactly 7M issued → Phase 2: 35 VRAM/window
        let rate = reward_distributor::current_emission_rate(
            70_000_000_000,
            7_000_000_000_000_000,
        );
        assert!(rate == 35_000_000_000, 0);
    }

    #[test]
    fun test_emission_between_halvings() {
        // 8M issued → still Phase 2
        let rate = reward_distributor::current_emission_rate(
            70_000_000_000,
            8_000_000_000_000_000,
        );
        assert!(rate == 35_000_000_000, 0);
    }

    #[test]
    fun test_emission_just_before_cap() {
        // 1 unit before 10.5M → still Phase 2
        let rate = reward_distributor::current_emission_rate(
            70_000_000_000,
            10_499_999_999_999_999,
        );
        assert!(rate == 35_000_000_000, 0);
    }

    // ── can_emit ───────────────────────────────────────────────────────────────

    #[test]
    fun test_can_emit_at_zero() {
        assert!(reward_distributor::can_emit(0), 0);
    }

    #[test]
    fun test_can_emit_one_before_cap() {
        // 1 raw unit before 10.5M mining cap
        assert!(reward_distributor::can_emit(10_499_999_999_999_999), 0);
    }

    #[test]
    fun test_cannot_emit_at_cap() {
        // exactly at 10.5M → exhausted
        assert!(!reward_distributor::can_emit(10_500_000_000_000_000), 0);
    }

    #[test]
    fun test_cannot_emit_above_cap() {
        assert!(!reward_distributor::can_emit(10_500_000_000_000_001), 0);
    }
}
