// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

/// # Hparams (v0.5)
///
/// On-chain hyperparameters for the VRAM coordination layer.
/// All values are governance-updatable via HparamsAdminCap.
///
/// ## Tokenomics constants (locked — TAO-like fair launch)
///
/// Hard cap:            21,000,000 VRAM (9 decimals)
/// Mining allocation:   10,500,000 VRAM (50% — emitted per window, never pre-minted)
/// Genesis emission:    70 VRAM / window  →  10,080 VRAM / day
/// Halving:             Supply-based — triggers at 7M issued; single halving (TRIGGER_2 == cap)
/// Per-window split:    Testnet: 10000/0/0. Mainnet: 5000/2000/3000 (miner/validator/treasury).

module slcl::hparams {
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;

    const E_UNAUTHORIZED: u64 = 1;
    const E_BPS_MUST_SUM_10000: u64 = 2;

    // ── Default emission ───────────────────────────────────────────────────────

    /// 70 VRAM per window with 9 decimals.
    const EMISSION_PER_WINDOW_DEFAULT: u64 = 70_000_000_000;

    // ── Default supply caps ────────────────────────────────────────────────────

    /// 21,000,000 VRAM with 9 decimals.
    const MAX_SUPPLY_DEFAULT: u64 = 21_000_000_000_000_000;

    /// 10,500,000 VRAM — 50% of 21M hard cap.
    /// Emitted per window by reward_distributor. Never pre-minted.
    const MINING_ALLOCATION_DEFAULT: u64 = 10_500_000_000_000_000;

    // ── Default halving triggers (supply-based, not time-based) ───────────────

    /// First halving fires when 7M mining tokens have been issued.
    /// Emission: 70 → 35 VRAM/window.
    const HALVING_TRIGGER_1_DEFAULT: u64 = 7_000_000_000_000_000;

    /// Set equal to MINING_ALLOCATION so can_emit() returns false before this trigger fires.
    /// Effectively a single halving at 7M issued.
    const HALVING_TRIGGER_2_DEFAULT: u64 = 10_500_000_000_000_000;

    // ── Default validator bonding curve ───────────────────────────────────────
    // Permanent burn to enter. No refund on exit.
    // Validator receives a soulbound non-transferable ValidatorTicket.

    const VALIDATOR_BURN_TIER_1_DEFAULT: u64 =  2_100_000_000_000; // slots 1-25:    2,100 VRAM
    const VALIDATOR_BURN_TIER_2_DEFAULT: u64 =  4_200_000_000_000; // slots 26-100:  4,200 VRAM
    const VALIDATOR_BURN_TIER_3_DEFAULT: u64 = 10_500_000_000_000; // slots 101-250: 10,500 VRAM
    const VALIDATOR_BURN_TIER_4_DEFAULT: u64 = 21_000_000_000_000; // slots 251-500: 21,000 VRAM
    const MAX_VALIDATORS_DEFAULT: u64        = 500;

    // ── Default testnet early bonus ────────────────────────────────────────────

    /// 2x multiplier on contribution points for the first 90 days of testnet.
    const EARLY_BONUS_MULTIPLIER_DEFAULT: u64  = 2;
    const EARLY_BONUS_DURATION_MS_DEFAULT: u64 = 7_776_000_000; // 90 days in ms

    // ── Struct ─────────────────────────────────────────────────────────────────

    public struct Hparams has key {
        id: UID,

        // --- Training coordination ---
        window_duration_ms:   u64,
        put_window_open_ms:   u64,
        topk_compression:     u32,
        top_g:                u32,
        validator_offset:     u32,
        min_miner_stake:      u64,
        min_validator_stake:  u64,
        openskill_beta_fp:    u64,
        openskill_tau_fp:     u64,
        gauntlet_gamma_fp:    u64,
        sync_threshold:       u32,
        checkpoint_frequency: u32,

        // --- Emission ---
        emission_per_window: u64,
        /// Basis points out of 10000.
        /// Testnet default: 10000 / 0 / 0 (100% miners, points only).
        /// Mainnet: 5000 / 2000 / 3000 (miner / validator / treasury).
        miner_bps:           u64,
        validator_bps:       u64,
        treasury_bps:        u64,

        // --- Supply ---
        max_supply:          u64,
        mining_allocation:   u64,

        // --- Supply-based halving ---
        halving_trigger_1: u64,
        halving_trigger_2: u64,

        // --- Validator bonding curve ---
        validator_burn_tier_1: u64,
        validator_burn_tier_2: u64,
        validator_burn_tier_3: u64,
        validator_burn_tier_4: u64,
        max_validators:        u64,

        // --- Testnet early bonus ---
        early_bonus_multiplier:  u64,
        early_bonus_duration_ms: u64,

        // --- Enclave PCR values (set after enclave build) ---
        expected_pcr0: vector<u8>,
        expected_pcr1: vector<u8>,
        expected_pcr2: vector<u8>,

        admin: address,
    }

    public struct HparamsAdminCap has key, store { id: UID }

    fun init(ctx: &mut TxContext) {
        let hparams = Hparams {
            id: object::new(ctx),

            window_duration_ms:   600_000,
            put_window_open_ms:   480_000,
            topk_compression:     32,
            top_g:                15,
            validator_offset:     2,
            min_miner_stake:      1_000_000_000,
            min_validator_stake:  10_000_000_000,
            openskill_beta_fp:    4_166_666_666,   // 25/6 × 1e9
            openskill_tau_fp:     83_333_333,       // 25/300 × 1e9
            gauntlet_gamma_fp:    990_000_000,      // 0.99 × 1e9
            sync_threshold:       3,
            checkpoint_frequency: 100,

            emission_per_window:  EMISSION_PER_WINDOW_DEFAULT,
            miner_bps:            10_000, // testnet: 100% to miners as points
            validator_bps:        0,
            treasury_bps:         0,

            max_supply:           MAX_SUPPLY_DEFAULT,
            mining_allocation:    MINING_ALLOCATION_DEFAULT,

            halving_trigger_1:    HALVING_TRIGGER_1_DEFAULT,
            halving_trigger_2:    HALVING_TRIGGER_2_DEFAULT,

            validator_burn_tier_1: VALIDATOR_BURN_TIER_1_DEFAULT,
            validator_burn_tier_2: VALIDATOR_BURN_TIER_2_DEFAULT,
            validator_burn_tier_3: VALIDATOR_BURN_TIER_3_DEFAULT,
            validator_burn_tier_4: VALIDATOR_BURN_TIER_4_DEFAULT,
            max_validators:        MAX_VALIDATORS_DEFAULT,

            early_bonus_multiplier:  EARLY_BONUS_MULTIPLIER_DEFAULT,
            early_bonus_duration_ms: EARLY_BONUS_DURATION_MS_DEFAULT,

            expected_pcr0: vector::empty(),
            expected_pcr1: vector::empty(),
            expected_pcr2: vector::empty(),

            admin: tx_context::sender(ctx),
        };
        transfer::share_object(hparams);
        transfer::transfer(
            HparamsAdminCap { id: object::new(ctx) },
            tx_context::sender(ctx),
        );
    }

    #[test_only]
    public fun init_for_testing(ctx: &mut TxContext) { init(ctx) }

    // ── Getters ────────────────────────────────────────────────────────────────

    public fun window_duration_ms(h: &Hparams): u64    { h.window_duration_ms }
    public fun put_window_open_ms(h: &Hparams): u64    { h.put_window_open_ms }
    public fun topk_compression(h: &Hparams): u32      { h.topk_compression }
    public fun top_g(h: &Hparams): u32                 { h.top_g }
    public fun validator_offset(h: &Hparams): u32      { h.validator_offset }
    public fun min_miner_stake(h: &Hparams): u64       { h.min_miner_stake }
    public fun min_validator_stake(h: &Hparams): u64   { h.min_validator_stake }
    public fun openskill_beta_fp(h: &Hparams): u64     { h.openskill_beta_fp }
    public fun sync_threshold(h: &Hparams): u32        { h.sync_threshold }
    public fun checkpoint_frequency(h: &Hparams): u32  { h.checkpoint_frequency }

    public fun emission_per_window(h: &Hparams): u64   { h.emission_per_window }
    public fun miner_bps(h: &Hparams): u64             { h.miner_bps }
    public fun validator_bps(h: &Hparams): u64         { h.validator_bps }
    public fun treasury_bps(h: &Hparams): u64          { h.treasury_bps }

    public fun max_supply(h: &Hparams): u64            { h.max_supply }
    public fun mining_allocation(h: &Hparams): u64     { h.mining_allocation }
    public fun halving_trigger_1(h: &Hparams): u64     { h.halving_trigger_1 }
    public fun halving_trigger_2(h: &Hparams): u64     { h.halving_trigger_2 }

    public fun validator_burn_tier_1(h: &Hparams): u64 { h.validator_burn_tier_1 }
    public fun validator_burn_tier_2(h: &Hparams): u64 { h.validator_burn_tier_2 }
    public fun validator_burn_tier_3(h: &Hparams): u64 { h.validator_burn_tier_3 }
    public fun validator_burn_tier_4(h: &Hparams): u64 { h.validator_burn_tier_4 }
    public fun max_validators(h: &Hparams): u64        { h.max_validators }

    public fun early_bonus_multiplier(h: &Hparams): u64  { h.early_bonus_multiplier }
    public fun early_bonus_duration_ms(h: &Hparams): u64 { h.early_bonus_duration_ms }

    public fun expected_pcr0(h: &Hparams): vector<u8>  { h.expected_pcr0 }
    public fun expected_pcr1(h: &Hparams): vector<u8>  { h.expected_pcr1 }
    public fun expected_pcr2(h: &Hparams): vector<u8>  { h.expected_pcr2 }

    // ── Governance ─────────────────────────────────────────────────────────────

    /// Update enclave PCR values when a new approved enclave binary is deployed.
    public entry fun update_pcrs(
        h: &mut Hparams,
        _cap: &HparamsAdminCap,
        pcr0: vector<u8>,
        pcr1: vector<u8>,
        pcr2: vector<u8>,
    ) {
        h.expected_pcr0 = pcr0;
        h.expected_pcr1 = pcr1;
        h.expected_pcr2 = pcr2;
    }

    /// Update the emission rate (called at each halving event).
    public entry fun update_emission(
        h: &mut Hparams,
        _cap: &HparamsAdminCap,
        emission_per_window: u64,
    ) {
        h.emission_per_window = emission_per_window;
    }

    /// Switch the per-window split from testnet (100/0/0) to mainnet (50/20/30).
    /// Called once at TGE. Basis points must sum to exactly 10000.
    public entry fun update_emission_split(
        h: &mut Hparams,
        _cap: &HparamsAdminCap,
        miner_bps: u64,
        validator_bps: u64,
        treasury_bps: u64,
    ) {
        assert!(miner_bps + validator_bps + treasury_bps == 10_000, E_BPS_MUST_SUM_10000);
        h.miner_bps    = miner_bps;
        h.validator_bps = validator_bps;
        h.treasury_bps  = treasury_bps;
    }

    /// Update supply-based halving triggers.
    public entry fun update_halving_triggers(
        h: &mut Hparams,
        _cap: &HparamsAdminCap,
        trigger_1: u64,
        trigger_2: u64,
    ) {
        h.halving_trigger_1 = trigger_1;
        h.halving_trigger_2 = trigger_2;
    }
}
