// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

/// # ValidatorRegistry (v0.5)
///
/// Tracks staked validators and enforces the bonding curve burn mechanic.
///
/// ## Bonding curve
///
/// Validator entry requires permanently burning $VRAM.
/// The burn is irreversible — no refund on exit.
/// Validators receive a soulbound, non-transferable ValidatorTicket.
///
/// Slots  1-25:   burn  2,100 VRAM
/// Slots 26-100:  burn  4,200 VRAM
/// Slots 101-250: burn 10,500 VRAM
/// Slots 251-500: burn 21,000 VRAM
///
/// Maximum 500 validators. At capacity: ~7.2M VRAM burned (34% of 21M supply).
/// Burn amounts mirror hparams.move VALIDATOR_BURN_TIER_*_DEFAULT constants.
/// Updatable post-deploy via governance (update_burn_tiers).
///
/// ## Usage by seal_policy
///
/// seal_policy::seal_approve reads is_registered_validator to gate
/// credential decryption. Only staked, active validators with a
/// ValidatorTicket can decrypt miner R2 credentials.

module slcl::validator_registry {
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;
    use sui::table::{Self, Table};
    use sui::coin::{Self, Coin};
    use sui::balance::{Self, Balance};

    const E_NOT_REGISTERED: u64       = 1;
    const E_ALREADY_REGISTERED: u64   = 2;
    const E_INSUFFICIENT_STAKE: u64   = 3;
    const E_UNAUTHORIZED: u64         = 4;
    const E_MAX_VALIDATORS_REACHED: u64 = 404;
    const E_INSUFFICIENT_BURN: u64    = 405;

    // ── Bonding curve slot thresholds (immutable) ─────────────────────────────

    const MAX_VALIDATORS: u64  = 500;
    const TIER_2_THRESHOLD: u64 =  25;
    const TIER_3_THRESHOLD: u64 = 100;
    const TIER_4_THRESHOLD: u64 = 250;

    // ── Default burn amounts (used at create_registry, updatable by governance) ─
    // Mirrors hparams.move VALIDATOR_BURN_TIER_*_DEFAULT (kept in sync manually).
    // At full capacity: ~7.2M VRAM burned = 34% of 21M supply.

    const DEFAULT_BURN_TIER_1: u64 =   2_100_000_000_000; // slots 1-25:    2,100 VRAM
    const DEFAULT_BURN_TIER_2: u64 =   4_200_000_000_000; // slots 26-100:  4,200 VRAM
    const DEFAULT_BURN_TIER_3: u64 =  10_500_000_000_000; // slots 101-250: 10,500 VRAM
    const DEFAULT_BURN_TIER_4: u64 =  21_000_000_000_000; // slots 251-500: 21,000 VRAM

    // ── Structs ────────────────────────────────────────────────────────────────

    public struct ValidatorRegistry<phantom T> has key {
        id: UID,
        validators: Table<address, ValidatorRecord>,
        min_stake: u64,
        admin: address,
        /// Accumulates permanently burned tokens from the bonding curve.
        /// No withdrawal function exists — this balance is locked forever.
        burn_vault: Balance<T>,
        /// Running validator slot counter (never decremented on exit).
        validator_count: u64,
        /// Bonding curve burn amounts — governance-updatable without contract upgrade.
        burn_tier_1: u64,
        burn_tier_2: u64,
        burn_tier_3: u64,
        burn_tier_4: u64,
    }

    /// Per-validator on-chain record.
    public struct ValidatorRecord has store, drop {
        addr: address,
        uid: u64,
        stake: u64,
        is_active: bool,
        slash_count: u64,
        burn_tier: u8,         // 1-4: which bonding curve tier was paid
        burned_amount: u64,    // exact amount burned at registration
    }

    /// Soulbound registration credential.
    ///
    /// `has key` but NOT `has store` — this object cannot be transferred
    /// using transfer::public_transfer. It is bound to the registering address
    /// and cannot be sold or delegated.
    ///
    /// The ValidatorTicket authorizes:
    ///   - Seal IBE decryption (via seal_policy::seal_approve)
    ///   - Score submission to score_ledger.move
    public struct ValidatorTicket has key {
        id: UID,
        validator_uid: u64,
        owner: address,
        burn_tier: u8,
        burned_amount: u64,
        registered_at_ms: u64,
    }

    public struct RegistryAdminCap has key, store { id: UID }

    fun init(ctx: &mut TxContext) {
        transfer::transfer(
            RegistryAdminCap { id: object::new(ctx) },
            tx_context::sender(ctx),
        );
    }

    /// Create the validator registry for token type T (called by deployer at launch).
    public entry fun create_registry<T>(
        _cap: &RegistryAdminCap,
        min_stake: u64,
        ctx: &mut TxContext,
    ) {
        transfer::share_object(ValidatorRegistry<T> {
            id: object::new(ctx),
            validators: table::new(ctx),
            min_stake,
            admin: tx_context::sender(ctx),
            burn_vault: balance::zero<T>(),
            validator_count: 0,
            burn_tier_1: DEFAULT_BURN_TIER_1,
            burn_tier_2: DEFAULT_BURN_TIER_2,
            burn_tier_3: DEFAULT_BURN_TIER_3,
            burn_tier_4: DEFAULT_BURN_TIER_4,
        });
    }

    #[test_only]
    public fun init_for_testing(ctx: &mut TxContext) { init(ctx) }

    #[test_only]
    public fun create_registry_for_testing<T>(min_stake: u64, ctx: &mut TxContext) {
        transfer::share_object(ValidatorRegistry<T> {
            id: object::new(ctx),
            validators: table::new(ctx),
            min_stake,
            admin: tx_context::sender(ctx),
            burn_vault: balance::zero<T>(),
            validator_count: 0,
            burn_tier_1: DEFAULT_BURN_TIER_1,
            burn_tier_2: DEFAULT_BURN_TIER_2,
            burn_tier_3: DEFAULT_BURN_TIER_3,
            burn_tier_4: DEFAULT_BURN_TIER_4,
        });
    }

    #[test_only]
    public fun register_validator_for_testing<T>(
        registry: &mut ValidatorRegistry<T>,
        uid: u64,
        stake: u64,
        ctx: &mut TxContext,
    ) {
        let sender = tx_context::sender(ctx);
        table::add(&mut registry.validators, sender, ValidatorRecord {
            addr: sender,
            uid,
            stake,
            is_active: true,
            slash_count: 0,
            burn_tier: 1,
            burned_amount: 0,
        });
        registry.validator_count = registry.validator_count + 1;
    }

    // ── Bonding curve ──────────────────────────────────────────────────────────

    /// Returns the required burn amount for the next validator slot.
    /// Reads governance-updatable tier amounts from the registry.
    public fun get_burn_amount<T>(registry: &ValidatorRegistry<T>): u64 {
        let n = registry.validator_count;
        if (n >= MAX_VALIDATORS)         { abort E_MAX_VALIDATORS_REACHED }
        else if (n >= TIER_4_THRESHOLD)  { registry.burn_tier_4 }
        else if (n >= TIER_3_THRESHOLD)  { registry.burn_tier_3 }
        else if (n >= TIER_2_THRESHOLD)  { registry.burn_tier_2 }
        else                             { registry.burn_tier_1 }
    }

    fun burn_tier_number(n: u64): u8 {
        if (n >= TIER_4_THRESHOLD) { 4 }
        else if (n >= TIER_3_THRESHOLD) { 3 }
        else if (n >= TIER_2_THRESHOLD) { 2 }
        else { 1 }
    }

    // ── Registration ───────────────────────────────────────────────────────────

    /// Register a validator by permanently burning the bonding curve amount.
    ///
    /// The caller must pass exactly `get_burn_amount(current_count)` worth of
    /// VRAM tokens in `burn_coin`. Any excess is returned to the sender.
    ///
    /// On success, the caller receives a soulbound `ValidatorTicket`.
    /// The burned tokens are deposited into `burn_vault` — locked forever.
    ///
    /// The caller must also provide `stake >= min_stake` separately; stake is
    /// tracked on-chain but held off-chain (the stake is a self-reported signal
    /// used by seal_approve for access control — actual slashing is governance-controlled).
    public entry fun register_validator_with_burn<T>(
        registry: &mut ValidatorRegistry<T>,
        uid: u64,
        stake: u64,
        burn_coin: Coin<T>,
        clock_ms: u64,  // pass sui::clock::Clock.timestamp_ms() from PTB
        ctx: &mut TxContext,
    ) {
        let sender = tx_context::sender(ctx);

        assert!(!table::contains(&registry.validators, sender), E_ALREADY_REGISTERED);
        assert!(stake >= registry.min_stake, E_INSUFFICIENT_STAKE);
        assert!(registry.validator_count < MAX_VALIDATORS, E_MAX_VALIDATORS_REACHED);

        let required_burn = get_burn_amount(registry);
        assert!(coin::value(&burn_coin) >= required_burn, E_INSUFFICIENT_BURN);

        let tier = burn_tier_number(registry.validator_count);

        // Split out exactly the required burn amount.
        let mut burn_coin_mut = burn_coin;
        let exact_burn = coin::split(&mut burn_coin_mut, required_burn, ctx);

        // Permanently lock burned tokens in the vault (no withdrawal function).
        let burn_balance = coin::into_balance(exact_burn);
        balance::join(&mut registry.burn_vault, burn_balance);

        // Return any excess to sender.
        if (coin::value(&burn_coin_mut) > 0) {
            transfer::public_transfer(burn_coin_mut, sender);
        } else {
            coin::destroy_zero(burn_coin_mut);
        };

        // Record on-chain.
        table::add(&mut registry.validators, sender, ValidatorRecord {
            addr: sender,
            uid,
            stake,
            is_active: true,
            slash_count: 0,
            burn_tier: tier,
            burned_amount: required_burn,
        });

        registry.validator_count = registry.validator_count + 1;

        // Issue soulbound ticket to the caller.
        // ValidatorTicket has no 'store' ability, so transfer::public_transfer
        // is not available — only the module can transfer it, binding it to sender.
        transfer::transfer(ValidatorTicket {
            id: object::new(ctx),
            validator_uid: uid,
            owner: sender,
            burn_tier: tier,
            burned_amount: required_burn,
            registered_at_ms: clock_ms,
        }, sender);
    }

    // ── Queries ────────────────────────────────────────────────────────────────

    public fun is_registered_validator<T>(registry: &ValidatorRegistry<T>, addr: address): bool {
        table::contains(&registry.validators, addr) &&
        table::borrow(&registry.validators, addr).is_active
    }

    public fun get_stake<T>(registry: &ValidatorRegistry<T>, addr: address): u64 {
        assert!(table::contains(&registry.validators, addr), E_NOT_REGISTERED);
        table::borrow(&registry.validators, addr).stake
    }

    public fun min_stake<T>(registry: &ValidatorRegistry<T>): u64 {
        registry.min_stake
    }

    /// Total tokens permanently burned via the bonding curve (publicly auditable).
    public fun burn_vault_balance<T>(registry: &ValidatorRegistry<T>): u64 {
        balance::value(&registry.burn_vault)
    }

    /// Current number of registered validator slots used (0..=500).
    public fun validator_count<T>(registry: &ValidatorRegistry<T>): u64 {
        registry.validator_count
    }

    /// Remaining open validator slots.
    public fun slots_remaining<T>(registry: &ValidatorRegistry<T>): u64 {
        if (registry.validator_count >= MAX_VALIDATORS) { 0 }
        else { MAX_VALIDATORS - registry.validator_count }
    }

    // ── Governance ─────────────────────────────────────────────────────────────

    /// Slash a validator (reduce stake, increment slash count).
    public entry fun slash_validator<T>(
        registry: &mut ValidatorRegistry<T>,
        _cap: &RegistryAdminCap,
        addr: address,
        slash_amount: u64,
    ) {
        assert!(table::contains(&registry.validators, addr), E_NOT_REGISTERED);
        let record = table::borrow_mut(&mut registry.validators, addr);
        if (record.stake > slash_amount) {
            record.stake = record.stake - slash_amount;
        } else {
            record.stake = 0;
            record.is_active = false;
        };
        record.slash_count = record.slash_count + 1;
    }

    /// Update bonding curve burn amounts without a contract upgrade.
    /// All four tiers must be supplied; they must be non-zero and tier1 ≤ tier2 ≤ tier3 ≤ tier4.
    public entry fun update_burn_tiers<T>(
        registry: &mut ValidatorRegistry<T>,
        _cap: &RegistryAdminCap,
        tier1: u64,
        tier2: u64,
        tier3: u64,
        tier4: u64,
    ) {
        assert!(tier1 > 0 && tier1 <= tier2 && tier2 <= tier3 && tier3 <= tier4, 406);
        registry.burn_tier_1 = tier1;
        registry.burn_tier_2 = tier2;
        registry.burn_tier_3 = tier3;
        registry.burn_tier_4 = tier4;
    }

    /// Expose current burn tiers for off-chain display.
    public fun burn_tiers<T>(registry: &ValidatorRegistry<T>): (u64, u64, u64, u64) {
        (registry.burn_tier_1, registry.burn_tier_2, registry.burn_tier_3, registry.burn_tier_4)
    }
}
