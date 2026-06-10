// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

/// # RewardDistributor (v0.5)
///
/// Distributes VRAM token rewards to peers after each training window.
/// Reward amounts are proportional to normalized OpenSkill weights from ScoreLedger.
///
/// ## Supply-based halving
///
/// Halving is triggered by cumulative mining tokens issued, not by time or block number.
/// The faster the network grows, the sooner each halving fires — early miners are
/// rewarded asymmetrically relative to late joiners.
///
/// Trigger 1:   7,000,000 VRAM issued  →  emission 70 → 35 VRAM/window (Phase 2)
/// Trigger 2:  10,500,000 VRAM issued  →  can_emit() returns false first; effectively
///             a single halving at 7M (HALVING_TRIGGER_2 == MINING_ALLOCATION).
/// Exhaustion: 10,500,000 VRAM issued  →  emission stops permanently
///
/// The 10.5M cap is 50% of the 21M hard cap (mining allocation).
/// The remaining 50% (10.5M) was pre-minted at TGE and is not affected by this contract.
///
/// ## Per-window split — trustless enforcement
///
/// The treasury cut is enforced on-chain. Before distributing to miners/validators,
/// `distribute_rewards` always carves out `treasury_bps / 10000` of the window
/// emission and sends it to the stored `treasury_address`. The aggregator cannot
/// route treasury funds elsewhere.
///
/// Testnet (current): treasury_bps = 0  (100% to miners as contribution points).
/// Mainnet: miner 5000 / validator 2000 / treasury 3000 bps (set via governance at TGE).
///
/// The miner/validator/treasury split is determined by normalized_weights + treasury_bps.
/// Per-window at mainnet: 35 VRAM miners / 14 VRAM validators / 21 VRAM treasury (out of 70).

module slcl::reward_distributor {
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;
    use sui::table::{Self, Table};
    use sui::coin::{Self, Coin};
    use sui::balance::{Self, Balance};

    const E_WINDOW_NOT_FINALIZED: u64    = 1;
    const E_REWARDS_ALREADY_DISTRIBUTED: u64 = 2;
    const E_UNAUTHORIZED: u64           = 3;
    const E_EMPTY_REWARD_POOL: u64      = 4;
    const E_WEIGHT_SUM_MISMATCH: u64    = 5;
    const E_MISMATCHED_INPUTS: u64      = 6;
    const E_MINING_ALLOCATION_EXHAUSTED: u64 = 7;
    const E_BPS_EXCEEDS_10000: u64           = 8;

    /// Fixed-point scale: 1e9 = 1.0
    const SCALE: u64 = 1_000_000_000;

    /// Basis-point denominator.
    const BPS_DENOM: u64 = 10_000;

    /// Per-window emission cap: 10,500,000 VRAM (50% of 21M hard cap).
    const MINING_ALLOCATION: u64 = 10_500_000_000_000_000;

    /// Supply-based halving triggers (cumulative mining tokens issued).
    const HALVING_TRIGGER_1: u64 = 7_000_000_000_000_000;
    const HALVING_TRIGGER_2: u64 = 10_500_000_000_000_000;

    // ── Structs ────────────────────────────────────────────────────────────────

    public struct RewardPool<phantom T> has key {
        id: UID,
        balance: Balance<T>,
        distributed_windows: Table<u64, bool>,
        /// Base emission rate (genesis: 70 VRAM = 70_000_000_000).
        /// Updated by governance at each halving. Used as the ceiling; the
        /// actual per-window emission is computed by current_emission_rate().
        emission_per_window: u64,
        /// Cumulative mining tokens issued. Used as the halving trigger counter.
        /// Incremented by distribute_rewards on every successful emission.
        mining_tokens_issued: u64,
        admin: address,
        /// Address that always receives the treasury cut before peer distribution.
        /// Set at pool creation; updatable via governance.
        treasury_address: address,
        /// Basis points (out of 10,000) carved out for the treasury every window.
        /// Testnet default: 0 (100% contribution points to miners, no real transfer).
        /// Mainnet: 3000 (set via governance at TGE — 30% per-window to treasury).
        /// The remaining (10,000 - treasury_bps) is split among peer_addresses
        /// by the aggregator according to miner/validator weights.
        treasury_bps: u64,
        /// Lifetime cumulative VRAM distributed to each peer address (miners + validators).
        /// Source of truth for the TGE airdrop snapshot — testnet earnings convert 1:1
        /// (or by configured ratio) into mainnet VRAM at launch. Survives RPC pruning;
        /// no event-replay dependency.
        cumulative_earned: Table<address, u64>,
    }

    public struct DistributorAdminCap has key, store { id: UID }

    fun init(ctx: &mut TxContext) {
        transfer::transfer(
            DistributorAdminCap { id: object::new(ctx) },
            tx_context::sender(ctx),
        );
    }

    #[test_only]
    public fun init_for_testing(ctx: &mut TxContext) { init(ctx) }

    // ── Halving logic ──────────────────────────────────────────────────────────

    /// Compute the current emission rate based on cumulative mining tokens issued.
    ///
    /// This is a pure function — it does not modify state.
    /// The halving is supply-based, not time-based. The more tokens issued,
    /// the lower the rate.
    ///
    ///   0 → 7M issued:    70 VRAM/window  (Phase 1 — genesis)
    ///   7M+ issued:       35 VRAM/window  (Phase 2 — first halving)
    ///   10.5M+ issued:     0              (exhausted — HALVING_TRIGGER_2 == MINING_ALLOCATION)
    public fun current_emission_rate(base: u64, mining_tokens_issued: u64): u64 {
        if (mining_tokens_issued >= HALVING_TRIGGER_2) {
            base / 4   // unreachable: can_emit() returns false first when TRIGGER_2 == MINING_ALLOCATION
        } else if (mining_tokens_issued >= HALVING_TRIGGER_1) {
            base / 2   // Phase 2: 35 VRAM/window   (7M → 10.5M)
        } else {
            base       // Phase 1: 70 VRAM/window   (0 → 7M)
        }
    }

    /// Returns false when the 10.5M per-window allocation is fully exhausted.
    /// No emission should occur after this point.
    public fun can_emit(mining_tokens_issued: u64): bool {
        mining_tokens_issued < MINING_ALLOCATION
    }

    // ── Pool management ────────────────────────────────────────────────────────

    public entry fun create_pool<T>(
        _cap: &DistributorAdminCap,
        emission_per_window: u64,
        treasury_address: address,
        treasury_bps: u64,
        ctx: &mut TxContext,
    ) {
        assert!(treasury_bps <= BPS_DENOM, E_BPS_EXCEEDS_10000);
        let pool = RewardPool<T> {
            id: object::new(ctx),
            balance: balance::zero<T>(),
            distributed_windows: table::new(ctx),
            emission_per_window,
            mining_tokens_issued: 0,
            admin: tx_context::sender(ctx),
            treasury_address,
            treasury_bps,
            cumulative_earned: table::new(ctx),
        };
        transfer::share_object(pool);
    }

    public entry fun deposit<T>(
        pool: &mut RewardPool<T>,
        coin: Coin<T>,
    ) {
        let deposited = coin::into_balance(coin);
        balance::join(&mut pool.balance, deposited);
    }

    // ── Distribution ───────────────────────────────────────────────────────────

    /// Distribute rewards for a completed window.
    ///
    /// `peer_uids`:          list of peer UIDs to receive rewards (miners + validators)
    /// `normalized_weights`: fixed-point weights (scale=1e9) summing to 1e9,
    ///                       representing each peer's share of the non-treasury slice
    /// `peer_addresses`:     recipient address for each uid
    ///
    /// Treasury enforcement (trustless):
    ///   1. Carve out `treasury_bps / 10000` of the window emission.
    ///   2. Send it directly to `pool.treasury_address` — the aggregator cannot redirect it.
    ///   3. Distribute the remainder among `peer_addresses` by `normalized_weights`.
    ///
    /// Emission for this window is computed dynamically from `mining_tokens_issued`
    /// using the supply-based halving formula in `current_emission_rate`.
    ///
    /// If the mining allocation would be exceeded, the actual emission is capped
    /// at the remaining allocation.
    ///
    /// Each peer receives: floor(peer_emission * weight / SCALE) tokens.
    /// Any rounding remainder stays in the pool.
    public entry fun distribute_rewards<T>(
        pool: &mut RewardPool<T>,
        window: u64,
        peer_uids: vector<u64>,
        normalized_weights: vector<u64>,
        peer_addresses: vector<address>,
        ctx: &mut TxContext,
    ) {
        assert!(tx_context::sender(ctx) == pool.admin, E_UNAUTHORIZED);

        let n = vector::length(&peer_uids);
        assert!(n == vector::length(&normalized_weights), E_MISMATCHED_INPUTS);
        assert!(n == vector::length(&peer_addresses),     E_MISMATCHED_INPUTS);

        assert!(
            !table::contains(&pool.distributed_windows, window),
            E_REWARDS_ALREADY_DISTRIBUTED,
        );

        // Guard: stop entirely if mining allocation is exhausted.
        assert!(can_emit(pool.mining_tokens_issued), E_MINING_ALLOCATION_EXHAUSTED);

        // Validate peer weights sum to SCALE.
        let mut weight_sum = 0u64;
        let mut i = 0;
        while (i < n) {
            weight_sum = weight_sum + *vector::borrow(&normalized_weights, i);
            i = i + 1;
        };
        assert!(weight_sum == SCALE, E_WEIGHT_SUM_MISMATCH);

        // Compute emission for this window using supply-based halving.
        let base_emission = current_emission_rate(pool.emission_per_window, pool.mining_tokens_issued);

        // Cap emission at remaining mining allocation.
        let remaining = MINING_ALLOCATION - pool.mining_tokens_issued;
        let emission = if (base_emission > remaining) { remaining } else { base_emission };

        assert!(balance::value(&pool.balance) >= emission, E_EMPTY_REWARD_POOL);

        // ── Step 1: Treasury cut (trustless, always sent first) ────────────────
        //
        // treasury_amount = floor(emission * treasury_bps / 10_000)
        // This is carved out before peer distribution so the aggregator
        // cannot route it elsewhere.
        let treasury_amount = emission / BPS_DENOM * pool.treasury_bps
            + (emission % BPS_DENOM * pool.treasury_bps) / BPS_DENOM;

        if (treasury_amount > 0 && balance::value(&pool.balance) >= treasury_amount) {
            let treasury_balance = balance::split(&mut pool.balance, treasury_amount);
            let treasury_coin    = coin::from_balance(treasury_balance, ctx);
            transfer::public_transfer(treasury_coin, pool.treasury_address);
        };

        // ── Step 2: Peer distribution (miners + validators) ───────────────────
        //
        // peer_emission is the remainder after the treasury cut.
        // normalized_weights sum to SCALE and represent each peer's share
        // of this remainder (aggregator encodes miner/validator split here).
        let peer_emission = emission - treasury_amount;

        let mut j = 0;
        while (j < n) {
            let weight    = *vector::borrow(&normalized_weights, j);
            let recipient = *vector::borrow(&peer_addresses, j);

            let amount = peer_emission / SCALE * weight
                + (peer_emission % SCALE * weight) / SCALE;

            if (amount > 0 && balance::value(&pool.balance) >= amount) {
                let reward_balance = balance::split(&mut pool.balance, amount);
                let reward_coin    = coin::from_balance(reward_balance, ctx);
                transfer::public_transfer(reward_coin, recipient);

                if (table::contains(&pool.cumulative_earned, recipient)) {
                    let prior = table::borrow_mut(&mut pool.cumulative_earned, recipient);
                    *prior = *prior + amount;
                } else {
                    table::add(&mut pool.cumulative_earned, recipient, amount);
                };
            };

            j = j + 1;
        };

        // Update cumulative counter and mark window distributed.
        pool.mining_tokens_issued = pool.mining_tokens_issued + emission;
        table::add(&mut pool.distributed_windows, window, true);
    }

    // ── Governance ─────────────────────────────────────────────────────────────

    /// Update base emission per window (governance, called at halving events).
    public entry fun update_emission<T>(
        pool: &mut RewardPool<T>,
        _cap: &DistributorAdminCap,
        new_emission: u64,
    ) {
        pool.emission_per_window = new_emission;
    }

    /// Update the treasury address (governance).
    /// Called if the protocol treasury wallet rotates.
    public entry fun update_treasury_address<T>(
        pool: &mut RewardPool<T>,
        _cap: &DistributorAdminCap,
        new_address: address,
    ) {
        pool.treasury_address = new_address;
    }

    /// Update the treasury basis points (governance).
    /// Mainnet: 3000 bps (set via governance at TGE). `new_bps` must be <= 10,000.
    public entry fun update_treasury_bps<T>(
        pool: &mut RewardPool<T>,
        _cap: &DistributorAdminCap,
        new_bps: u64,
    ) {
        assert!(new_bps <= BPS_DENOM, E_BPS_EXCEEDS_10000);
        pool.treasury_bps = new_bps;
    }

    /// Transfer admin rights to a new address (governance, used at TGE to hand off to aggregator enclave).
    public entry fun transfer_admin<T>(
        pool: &mut RewardPool<T>,
        _cap: &DistributorAdminCap,
        new_admin: address,
    ) {
        pool.admin = new_admin;
    }

    // ── Queries ────────────────────────────────────────────────────────────────

    public fun is_distributed<T>(pool: &RewardPool<T>, window: u64): bool {
        table::contains(&pool.distributed_windows, window)
    }

    public fun pool_balance<T>(pool: &RewardPool<T>): u64 {
        balance::value(&pool.balance)
    }

    /// Cumulative mining tokens issued so far.
    public fun mining_tokens_issued<T>(pool: &RewardPool<T>): u64 {
        pool.mining_tokens_issued
    }

    /// Remaining per-window allocation before the 10.5M cap is reached.
    public fun mining_allocation_remaining<T>(pool: &RewardPool<T>): u64 {
        if (pool.mining_tokens_issued >= MINING_ALLOCATION) {
            0
        } else {
            MINING_ALLOCATION - pool.mining_tokens_issued
        }
    }

    public fun treasury_address<T>(pool: &RewardPool<T>): address {
        pool.treasury_address
    }

    public fun treasury_bps<T>(pool: &RewardPool<T>): u64 {
        pool.treasury_bps
    }

    /// Lifetime VRAM ever distributed to `addr` from this pool.
    /// Returns 0 if `addr` has never received a reward.
    /// Read at TGE to compute the testnet→mainnet airdrop snapshot.
    public fun cumulative_earned_of<T>(pool: &RewardPool<T>, addr: address): u64 {
        if (table::contains(&pool.cumulative_earned, addr)) {
            *table::borrow(&pool.cumulative_earned, addr)
        } else {
            0
        }
    }

    /// True iff `addr` has ever received a reward from this pool.
    /// Lets the airdrop script iterate the known-peer set off-chain
    /// without scanning every Sui address.
    public fun has_earned<T>(pool: &RewardPool<T>, addr: address): bool {
        table::contains(&pool.cumulative_earned, addr)
    }
}
