// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

/// # VramToken (v0.7)
///
/// Native reward token for the VRAM coordination layer.
/// One-time witness pattern: minted only via TreasuryCap (held by deployer).
///
/// ## Token Allocation (21,000,000 VRAM total)
///
/// Per-window emission via reward_distributor (never pre-minted):
///   Mining pool:  50%  10,500,000  Split per window: miners 50%, validators 20%, treasury 30%.
///                                  Cap tracked by reward_distributor.MINING_ALLOCATION.
///
/// Pre-minted at TGE, sent to deployer multisig / vesting contracts:
///   Treasury:  30%   6,300,000  6m cliff, 48m linear vest.
///   Team:       8%   1,680,000  12m cliff, 36m linear vest.
///   Liquidity:  7%   1,470,000  100% unlocked. Seeds Cetus VRAM/SUI pool at TGE.
///   Airdrop:    5%   1,050,000  Instant at TGE. Converts from testnet points.
///   Subtotal:  50%  10,500,000  Minted once via premint_tge_allocations().

module slcl::vram_token {
    use sui::coin::{Self, TreasuryCap};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;

    // ── Allocation constants (with 9 decimals) ─────────────────────────────────

    /// 10,500,000 VRAM — 50% of 21M hard cap.
    /// Emitted per window by reward_distributor. Never pre-minted.
    const MINING_ALLOCATION: u64     = 10_500_000_000_000_000;

    /// 6,300,000 VRAM — 30% of 21M. 6m cliff, 48m linear vest.
    const TREASURY_ALLOCATION: u64   =  6_300_000_000_000_000;

    /// 1,680,000 VRAM — 8% of 21M. 12m cliff, 36m linear vest.
    const TEAM_ALLOCATION: u64       =  1_680_000_000_000_000;

    /// 1,470,000 VRAM — 7% of 21M. 100% at TGE. Seeds Cetus VRAM/SUI pool.
    const LIQUIDITY_ALLOCATION: u64  =  1_470_000_000_000_000;

    /// 1,050,000 VRAM — 5% of 21M. Instant at TGE. Converts from testnet points.
    const AIRDROP_ALLOCATION: u64    =  1_050_000_000_000_000;

    // Sanity: MINING + TREASURY + TEAM + LIQUIDITY + AIRDROP = 21,000,000
    // Pre-minted total: 6.3M + 1.68M + 1.47M + 1.05M = 10.5M (50%)
    // Per-window total: 10.5M (50%) — emitted by reward_distributor over time.

    // ── Vesting schedule constants (in ms) ────────────────────────────────────

    const TEAM_CLIFF_MS: u64       = 31_536_000_000;  // 12 months
    const TEAM_VEST_MS: u64        = 94_608_000_000;  // 36 months linear
    const TREASURY_CLIFF_MS: u64   = 15_768_000_000;  // 6 months
    const TREASURY_VEST_MS: u64    = 126_144_000_000; // 48 months linear
    const AIRDROP_CLIFF_MS: u64    = 0;               // no cliff
    const AIRDROP_VEST_MS: u64     = 0;               // instant at TGE conversion

    // ── Testnet airdrop pool ───────────────────────────────────────────────────

    /// Maximum VRAM per address at TGE airdrop conversion (10% of airdrop pool = 105,000 VRAM).
    const AIRDROP_MAX_PER_ADDRESS: u64 = 105_000_000_000_000;

    // ── Coin ───────────────────────────────────────────────────────────────────

    /// One-time witness for coin creation.
    public struct VRAM_TOKEN has drop {}

    #[test_only]
    public fun init_for_testing(ctx: &mut TxContext) { init(VRAM_TOKEN {}, ctx) }

    fun init(witness: VRAM_TOKEN, ctx: &mut TxContext) {
        let (treasury_cap, metadata) = coin::create_currency(
            witness,
            9,
            b"VRAM",
            b"VRAM",
            b"VRAM distributed LLM training reward token",
            std::option::none(),
            ctx,
        );

        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury_cap, tx_context::sender(ctx));
    }

    /// Mint new VRAM tokens.
    /// Called by reward_distributor.move (per window) and by the deployer at TGE
    /// for the pre-minted allocations.
    public fun mint(
        cap: &mut TreasuryCap<VRAM_TOKEN>,
        amount: u64,
        ctx: &mut TxContext,
    ): sui::coin::Coin<VRAM_TOKEN> {
        coin::mint(cap, amount, ctx)
    }

    /// Permanently destroy VRAM tokens (validator bonding curve burns).
    public fun burn(
        cap: &mut TreasuryCap<VRAM_TOKEN>,
        coin: sui::coin::Coin<VRAM_TOKEN>,
    ) {
        coin::burn(cap, coin);
    }

    /// Mint all pre-minted TGE allocations in one atomic call.
    ///
    /// Sends treasury + team + liquidity + airdrop (10,500,000 VRAM = 50%)
    /// to `recipient` (deployer multisig / vesting orchestrator).
    /// TreasuryCap stays with deployer for ongoing per-window minting.
    public entry fun premint_tge_allocations(
        cap: &mut TreasuryCap<VRAM_TOKEN>,
        recipient: address,
        ctx: &mut TxContext,
    ) {
        let total = TREASURY_ALLOCATION + TEAM_ALLOCATION + LIQUIDITY_ALLOCATION + AIRDROP_ALLOCATION;
        let coin = coin::mint(cap, total, ctx);
        transfer::public_transfer(coin, recipient);
    }

    // ── Allocation getters ────────────────────────────────────────────────────

    public fun mining_allocation(): u64    { MINING_ALLOCATION }
    public fun treasury_allocation(): u64  { TREASURY_ALLOCATION }
    public fun team_allocation(): u64      { TEAM_ALLOCATION }
    public fun liquidity_allocation(): u64 { LIQUIDITY_ALLOCATION }
    public fun airdrop_allocation(): u64   { AIRDROP_ALLOCATION }
    public fun airdrop_max_per_address(): u64 { AIRDROP_MAX_PER_ADDRESS }
}
