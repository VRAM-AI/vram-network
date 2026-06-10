// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

/// # StakingPool (v0.7)
///
/// VRAM staking with lockup tiers and proportional reward distribution.
///
/// Stakers deposit VRAM and choose a lockup duration. Longer lockups receive
/// a higher weight multiplier, giving them a larger share of deposited rewards.
///
/// ## Lockup tiers
///
/// | Tier | Duration | Multiplier |
/// |------|----------|------------|
/// |  0   |  30 days |    1.0x    |
/// |  1   |  90 days |    1.2x    |
/// |  2   | 180 days |    1.5x    |
/// |  3   | 365 days |    2.0x    |
///
/// ## Reward flow
///
/// Rewards are deposited via `deposit_rewards` (called by the buyback executor
/// or governance). Each deposit increases `accumulated_reward_per_weight`.
/// Stakers claim their pro-rata share via `claim_rewards` or during `unstake`.
///
/// Early unstake (before lockup expires) forfeits 25% of principal.
/// The penalty stays in the pool, benefiting remaining stakers.

module slcl::staking_pool {
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;
    use sui::coin::{Self, Coin};
    use sui::balance::{Self, Balance};
    use sui::clock::{Self, Clock};

    const E_NOTHING_TO_CLAIM: u64 = 1;
    const E_INVALID_TIER:     u64 = 2;

    // Lockup durations in milliseconds
    const LOCKUP_30D:  u64 = 2_592_000_000;
    const LOCKUP_90D:  u64 = 7_776_000_000;
    const LOCKUP_180D: u64 = 15_552_000_000;
    const LOCKUP_365D: u64 = 31_536_000_000;

    // Weight multipliers in BPS (10_000 = 1.0x)
    const MULT_30D:  u64 = 10_000;
    const MULT_90D:  u64 = 12_000;
    const MULT_180D: u64 = 15_000;
    const MULT_365D: u64 = 20_000;
    const BPS_DENOM: u64 = 10_000;

    // Early unstake penalty: 25%
    const PENALTY_BPS: u64 = 2_500;

    // Precision scale for reward accumulator
    const ACC_SCALE: u128 = 1_000_000_000;

    // ── Structs ────────────────────────────────────────────────────────────────

    public struct StakingPool<phantom T> has key {
        id: UID,
        stake_balance: Balance<T>,
        reward_balance: Balance<T>,
        total_stake_weight: u64,
        /// Cumulative rewards per unit of stake weight (× ACC_SCALE).
        /// u128 prevents overflow at realistic stake + reward magnitudes.
        accumulated_reward_per_weight: u128,
    }

    /// Owned by the staker. Passed directly to claim/unstake.
    public struct StakerTicket<phantom T> has key {
        id: UID,
        amount: u64,
        weight: u64,
        lockup_end_ms: u64,
        /// Snapshot of accumulated_reward_per_weight × weight at stake time.
        /// Used to compute unclaimed rewards: pending - reward_debt.
        reward_debt: u128,
    }

    // ── Pool management ────────────────────────────────────────────────────────

    /// Create a new staking pool for token type T.
    public entry fun create_pool<T>(ctx: &mut TxContext) {
        transfer::share_object(StakingPool<T> {
            id: object::new(ctx),
            stake_balance: balance::zero<T>(),
            reward_balance: balance::zero<T>(),
            total_stake_weight: 0,
            accumulated_reward_per_weight: 0,
        });
    }

    // ── Staking ────────────────────────────────────────────────────────────────

    /// Stake tokens with a lockup tier (0-3). Transfers a StakerTicket to the caller.
    public entry fun stake<T>(
        pool:         &mut StakingPool<T>,
        coin:         Coin<T>,
        lockup_tier:  u8,
        clock:        &Clock,
        ctx:          &mut TxContext,
    ) {
        let (duration_ms, mult_bps) = lockup_params(lockup_tier);
        let amount = coin::value(&coin);
        balance::join(&mut pool.stake_balance, coin::into_balance(coin));

        let weight = amount / BPS_DENOM * mult_bps + (amount % BPS_DENOM * mult_bps) / BPS_DENOM;
        let reward_debt = (weight as u128) * pool.accumulated_reward_per_weight / ACC_SCALE;
        pool.total_stake_weight = pool.total_stake_weight + weight;

        transfer::transfer(StakerTicket<T> {
            id: object::new(ctx),
            amount,
            weight,
            lockup_end_ms: clock::timestamp_ms(clock) + duration_ms,
            reward_debt,
        }, tx_context::sender(ctx));
    }

    /// Deposit rewards into the pool. Updates the per-weight accumulator.
    /// Called by the buyback executor or governance after acquiring VRAM.
    public entry fun deposit_rewards<T>(pool: &mut StakingPool<T>, coin: Coin<T>) {
        let amount = coin::value(&coin);
        balance::join(&mut pool.reward_balance, coin::into_balance(coin));
        if (pool.total_stake_weight > 0) {
            let delta = (amount as u128) * ACC_SCALE / (pool.total_stake_weight as u128);
            pool.accumulated_reward_per_weight = pool.accumulated_reward_per_weight + delta;
        };
    }

    /// Claim accumulated rewards without unstaking.
    public entry fun claim_rewards<T>(
        pool:   &mut StakingPool<T>,
        ticket: &mut StakerTicket<T>,
        ctx:    &mut TxContext,
    ) {
        let (claimable, new_debt) = pending_rewards(pool, ticket);
        assert!(claimable > 0, E_NOTHING_TO_CLAIM);
        ticket.reward_debt = new_debt;
        let reward = balance::split(&mut pool.reward_balance, claimable);
        transfer::public_transfer(coin::from_balance(reward, ctx), tx_context::sender(ctx));
    }

    /// Unstake. Claims pending rewards first. Applies 25% penalty if lockup not expired.
    public entry fun unstake<T>(
        pool:   &mut StakingPool<T>,
        ticket: StakerTicket<T>,
        clock:  &Clock,
        ctx:    &mut TxContext,
    ) {
        let sender = tx_context::sender(ctx);
        let now_ms  = clock::timestamp_ms(clock);

        let (claimable, _) = pending_rewards(pool, &ticket);
        if (claimable > 0 && balance::value(&pool.reward_balance) >= claimable) {
            let reward = balance::split(&mut pool.reward_balance, claimable);
            transfer::public_transfer(coin::from_balance(reward, ctx), sender);
        };

        pool.total_stake_weight = if (pool.total_stake_weight > ticket.weight) {
            pool.total_stake_weight - ticket.weight
        } else {
            0
        };

        let return_amount = if (now_ms < ticket.lockup_end_ms) {
            let penalty = ticket.amount / BPS_DENOM * PENALTY_BPS
                + (ticket.amount % BPS_DENOM * PENALTY_BPS) / BPS_DENOM;
            ticket.amount - penalty
            // penalty stays in stake_balance, benefiting remaining stakers
        } else {
            ticket.amount
        };

        let StakerTicket { id, amount: _, weight: _, lockup_end_ms: _, reward_debt: _ } = ticket;
        object::delete(id);

        let stake = balance::split(&mut pool.stake_balance, return_amount);
        transfer::public_transfer(coin::from_balance(stake, ctx), sender);
    }

    // ── Internal ───────────────────────────────────────────────────────────────

    fun pending_rewards<T>(
        pool:   &StakingPool<T>,
        ticket: &StakerTicket<T>,
    ): (u64, u128) {
        let gross = (ticket.weight as u128) * pool.accumulated_reward_per_weight / ACC_SCALE;
        let claimable = if (gross > ticket.reward_debt) {
            ((gross - ticket.reward_debt) as u64)
        } else {
            0
        };
        (claimable, gross)
    }

    fun lockup_params(tier: u8): (u64, u64) {
        if      (tier == 0) { (LOCKUP_30D,  MULT_30D)  }
        else if (tier == 1) { (LOCKUP_90D,  MULT_90D)  }
        else if (tier == 2) { (LOCKUP_180D, MULT_180D) }
        else if (tier == 3) { (LOCKUP_365D, MULT_365D) }
        else                { abort E_INVALID_TIER }
    }

    // ── Queries ────────────────────────────────────────────────────────────────

    public fun pool_stake_balance<T>(pool: &StakingPool<T>): u64  { balance::value(&pool.stake_balance) }
    public fun pool_reward_balance<T>(pool: &StakingPool<T>): u64 { balance::value(&pool.reward_balance) }
    public fun total_stake_weight<T>(pool: &StakingPool<T>): u64  { pool.total_stake_weight }
    public fun ticket_amount<T>(t: &StakerTicket<T>): u64         { t.amount }
    public fun ticket_weight<T>(t: &StakerTicket<T>): u64         { t.weight }
    public fun ticket_lockup_end<T>(t: &StakerTicket<T>): u64     { t.lockup_end_ms }
}
