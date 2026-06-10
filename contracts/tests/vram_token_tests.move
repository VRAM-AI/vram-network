// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

#[test_only]
module slcl::vram_token_tests {
    use sui::test_scenario;
    use sui::coin::{Self, TreasuryCap, Coin};
    use slcl::vram_token::{Self, VRAM_TOKEN};

    #[test]
    fun test_premint_tge_mints_correct_total() {
        let deployer = @0xDEAD;
        let mut s = test_scenario::begin(deployer);

        test_scenario::next_tx(&mut s, deployer);
        { vram_token::init_for_testing(test_scenario::ctx(&mut s)); };

        test_scenario::next_tx(&mut s, deployer);
        {
            let mut cap: TreasuryCap<VRAM_TOKEN> = test_scenario::take_from_sender(&s);
            vram_token::premint_tge_allocations(
                &mut cap,
                deployer,
                test_scenario::ctx(&mut s),
            );
            test_scenario::return_to_sender(&s, cap);
        };

        // TGE pre-mint = treasury (30%) + team (8%) + liquidity (7%) + airdrop (5%) = 50% = 10.5M
        test_scenario::next_tx(&mut s, deployer);
        {
            let coin: Coin<VRAM_TOKEN> = test_scenario::take_from_sender(&s);
            let expected =
                vram_token::treasury_allocation()  +
                vram_token::team_allocation()      +
                vram_token::liquidity_allocation() +
                vram_token::airdrop_allocation();
            assert!(coin::value(&coin) == expected, 0);
            assert!(coin::value(&coin) == 10_500_000_000_000_000, 1); // 50% of 21M
            test_scenario::return_to_sender(&s, coin);
        };

        test_scenario::end(s);
    }

    #[test]
    fun test_mint_and_burn() {
        let deployer = @0xDEAD;
        let mut s = test_scenario::begin(deployer);

        test_scenario::next_tx(&mut s, deployer);
        { vram_token::init_for_testing(test_scenario::ctx(&mut s)); };

        test_scenario::next_tx(&mut s, deployer);
        {
            let mut cap: TreasuryCap<VRAM_TOKEN> = test_scenario::take_from_sender(&s);
            let coin = vram_token::mint(&mut cap, 1_000_000_000, test_scenario::ctx(&mut s));
            assert!(coin::value(&coin) == 1_000_000_000, 0);
            vram_token::burn(&mut cap, coin);
            assert!(coin::total_supply(&cap) == 0, 1);
            test_scenario::return_to_sender(&s, cap);
        };

        test_scenario::end(s);
    }

    #[test]
    fun test_allocation_getters_are_consistent() {
        // Mining = 50% of 21M hard cap
        assert!(vram_token::mining_allocation() == 10_500_000_000_000_000, 0);
        // Treasury = 30% of 21M
        assert!(vram_token::treasury_allocation() == 6_300_000_000_000_000, 1);
        // Airdrop max per address = 10% of airdrop pool (105,000 VRAM)
        assert!(vram_token::airdrop_max_per_address() * 10 == vram_token::airdrop_allocation(), 2);
        // Pre-minted TGE total = other 50%
        let premint =
            vram_token::treasury_allocation()  +
            vram_token::team_allocation()      +
            vram_token::liquidity_allocation() +
            vram_token::airdrop_allocation();
        assert!(premint == vram_token::mining_allocation(), 3);
    }
}
