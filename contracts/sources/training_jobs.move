// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

/// # TrainingJobBoard (v1.0)
///
/// Permissionless on-chain marketplace for distributed LLM training jobs.
///
/// ## State machine
///
///   STATUS_OPEN (0) ──► STATUS_CLAIMED (1) ──► STATUS_COMPLETED (2) ──► STATUS_SETTLED (3)
///                                                     │
///                                                     └──► STATUS_DISPUTED (4) ──► STATUS_SETTLED / STATUS_REFUNDED
///
///   STATUS_OPEN     ──► STATUS_REFUNDED (5)   (cancel_job by customer, or refund_job after deadline)
///   STATUS_CLAIMED  ──► STATUS_REFUNDED (5)   (refund_job after deadline)
///
/// ## Pricing formula
///
///   compute_units = model_params_m × dataset_tokens_m × num_epochs / 1_000
///   adjusted_units = compute_units / 2   (INT8 only)
///   raw_price = adjusted_units × price_per_unit
///   miner_payout = max(raw_price, min_price)
///   protocol_fee = miner_payout × protocol_fee_bps / 10_000
///   customer_pays = miner_payout + protocol_fee

module slcl::training_jobs {
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;
    use sui::coin::{Self, Coin};
    use sui::balance::{Self, Balance};
    use sui::clock::{Self, Clock};
    use sui::table::{Self, Table};
    use sui::event;
    use std::string::String;
    use std::option::{Self, Option};
    use sui::sui::SUI;

    // ── Error codes ────────────────────────────────────────────────────────────

    const E_INVALID_PARAMS:        u64 = 1;
    const E_INSUFFICIENT_PAYMENT:  u64 = 2;
    const E_NOT_OPEN:              u64 = 3;
    const E_NOT_CLAIMED:           u64 = 4;
    const E_NOT_ASSIGNED_MINER:    u64 = 5;
    const E_DEADLINE_NOT_PASSED:   u64 = 6;
    const E_PAST_DEADLINE:         u64 = 7;
    const E_DISPUTE_WINDOW_ACTIVE: u64 = 8;
    const E_DISPUTE_WINDOW_PASSED: u64 = 9;
    const E_NOT_CUSTOMER:          u64 = 10;
    const E_NOT_COMPLETED:         u64 = 11;
    const E_UNAUTHORIZED:          u64 = 12;
    const E_NOT_DISPUTED:          u64 = 13;
    #[allow(unused_const)]
    const E_JOB_NOT_FOUND:         u64 = 14;

    // ── Status codes ───────────────────────────────────────────────────────────

    const STATUS_OPEN:      u8 = 0;
    const STATUS_CLAIMED:   u8 = 1;
    const STATUS_COMPLETED: u8 = 2;
    const STATUS_SETTLED:   u8 = 3;
    const STATUS_DISPUTED:  u8 = 4;
    const STATUS_REFUNDED:  u8 = 5;

    // ── Precision codes ────────────────────────────────────────────────────────

    const PRECISION_FP32: u8 = 0;
    const PRECISION_FP16: u8 = 1;
    const PRECISION_BF16: u8 = 2;
    const PRECISION_INT8: u8 = 3;

    // ── Parameter caps ─────────────────────────────────────────────────────────

    /// Maximum model size in millions of parameters (10T params)
    const MAX_MODEL_PARAMS_M:    u64 = 10_000_000;
    /// Maximum dataset size in millions of tokens (10T tokens)
    const MAX_DATASET_TOKENS_M:  u64 = 10_000_000;
    /// Maximum training epochs
    const MAX_EPOCHS:            u64 = 1_000;

    // ── Defaults ───────────────────────────────────────────────────────────────

    const DEFAULT_PRICE_PER_UNIT:    u64 = 10_000;
    const DEFAULT_PROTOCOL_FEE_BPS:  u64 = 500;
    const DEFAULT_MIN_PRICE:         u64 = 1_000_000_000;   // 1 SUI (9 decimals)
    const DEFAULT_DISPUTE_WINDOW_MS: u64 = 7_200_000;       // 2 hours
    const MIN_OPEN_DURATION_MS:      u64 = 3_600_000;       // 1 hour
    const MAX_OPEN_DURATION_MS:      u64 = 604_800_000;     // 7 days
    const BPS_DENOM:                 u64 = 10_000;

    // ── Structs ────────────────────────────────────────────────────────────────

    /// Shared singleton — holds all jobs and protocol configuration.
    public struct TrainingJobBoard has key {
        id: UID,
        jobs: Table<u64, TrainingJob>,
        job_counter: u64,
        price_per_unit: u64,
        protocol_fee_bps: u64,
        min_price: u64,
        dispute_window_ms: u64,
        /// Protocol fees accumulate here until swept by admin.
        fee_vault: Balance<SUI>,
        treasury: address,
        admin: address,
    }

    /// A single training job stored in the board's table.
    /// Has `store` but NOT `drop` — Balance<SUI> prevents drop.
    public struct TrainingJob has store {
        id: u64,
        customer: address,
        // ── Job specification ─────────────────────────────────────────────
        model_params_m: u64,
        dataset_tokens_m: u64,
        num_epochs: u32,
        batch_size: u32,
        sequence_length: u32,
        precision: u8,
        min_gpu_vram_gb: u32,
        dataset_blob_id: String,   // "walrus:{blob_id}"
        base_model_blob_id: String, // "" if not specified
        // ── Payment ───────────────────────────────────────────────────────
        escrow: Balance<SUI>,
        miner_payout: u64,
        protocol_fee: u64,
        // ── Lifecycle timestamps ──────────────────────────────────────────
        posted_at_ms: u64,
        deadline_ms: u64,
        claimed_at_ms: u64,
        completed_at_ms: u64,
        // ── Status ────────────────────────────────────────────────────────
        status: u8,
        miner_address: Option<address>,
        miner_uid: u64,
        // ── Result ────────────────────────────────────────────────────────
        result_blob_id: String,
        result_hash: vector<u8>,
    }

    /// Admin capability — holds key governance powers.
    public struct JobBoardAdminCap has key, store {
        id: UID,
    }

    // ── Events ─────────────────────────────────────────────────────────────────

    public struct JobPosted has copy, drop {
        job_id: u64,
        customer: address,
        miner_payout: u64,
        protocol_fee: u64,
        dataset_blob_id: String,
        deadline_ms: u64,
    }

    public struct JobClaimed has copy, drop {
        job_id: u64,
        miner: address,
        miner_uid: u64,
        deadline_ms: u64,
    }

    public struct JobCompleted has copy, drop {
        job_id: u64,
        miner: address,
        result_blob_id: String,
        completed_at_ms: u64,
    }

    public struct PaymentSettled has copy, drop {
        job_id: u64,
        miner: address,
        amount: u64,
    }

    public struct JobDisputed has copy, drop {
        job_id: u64,
        customer: address,
    }

    public struct DisputeResolved has copy, drop {
        job_id: u64,
        paid_miner: bool,
    }

    public struct JobRefunded has copy, drop {
        job_id: u64,
        customer: address,
        amount: u64,
    }

    public struct JobCancelled has copy, drop {
        job_id: u64,
        customer: address,
        amount: u64,
    }

    // ── Init ───────────────────────────────────────────────────────────────────

    fun init(ctx: &mut TxContext) {
        let sender = tx_context::sender(ctx);
        let board = TrainingJobBoard {
            id: object::new(ctx),
            jobs: table::new(ctx),
            job_counter: 0,
            price_per_unit: DEFAULT_PRICE_PER_UNIT,
            protocol_fee_bps: DEFAULT_PROTOCOL_FEE_BPS,
            min_price: DEFAULT_MIN_PRICE,
            dispute_window_ms: DEFAULT_DISPUTE_WINDOW_MS,
            fee_vault: balance::zero(),
            treasury: sender,
            admin: sender,
        };
        transfer::share_object(board);
        transfer::transfer(JobBoardAdminCap { id: object::new(ctx) }, sender);
    }

    #[test_only]
    public fun init_for_testing(ctx: &mut TxContext) {
        init(ctx)
    }

    // ── Price computation ──────────────────────────────────────────────────────

    /// Pure price calculation. Returns (miner_payout, protocol_fee).
    ///
    /// compute_units = model_params_m × dataset_tokens_m × num_epochs / 1_000
    /// INT8 precision halves the compute units.
    /// miner_payout = max(adjusted_units × price_per_unit, min_price)
    public fun compute_price(
        board: &TrainingJobBoard,
        model_params_m: u64,
        dataset_tokens_m: u64,
        num_epochs: u64,
        precision: u8,
    ): (u64, u64) {
        let compute_units = model_params_m * dataset_tokens_m * num_epochs / 1_000;
        let adjusted_units = if (precision == PRECISION_INT8) {
            compute_units / 2
        } else {
            compute_units
        };
        let raw_price = adjusted_units * board.price_per_unit;
        let miner_payout = if (raw_price > board.min_price) { raw_price } else { board.min_price };
        let protocol_fee = miner_payout * board.protocol_fee_bps / BPS_DENOM;
        (miner_payout, protocol_fee)
    }

    // ── Internal drain helpers (avoid simultaneous &mut borrow conflicts) ──────

    /// Splits miner_payout + protocol_fee out of job.escrow atomically.
    /// Sets job.status = STATUS_SETTLED.
    /// Returns (miner_balance, fee_balance).
    fun drain_job_for_settlement(
        board: &mut TrainingJobBoard,
        job_id: u64,
    ): (Balance<SUI>, Balance<SUI>) {
        let job = table::borrow_mut(&mut board.jobs, job_id);
        let miner_bal = balance::split(&mut job.escrow, job.miner_payout);
        let fee_bal   = balance::split(&mut job.escrow, job.protocol_fee);
        job.status = STATUS_SETTLED;
        (miner_bal, fee_bal)
    }

    /// Drains the full escrow and sets job.status to new_status.
    fun drain_job_full_refund(
        board: &mut TrainingJobBoard,
        job_id: u64,
        new_status: u8,
    ): Balance<SUI> {
        let job = table::borrow_mut(&mut board.jobs, job_id);
        let total = balance::value(&job.escrow);
        let bal = balance::split(&mut job.escrow, total);
        job.status = new_status;
        bal
    }

    // ── Job lifecycle ──────────────────────────────────────────────────────────

    /// Post a new training job. Customer deposits miner_payout + protocol_fee.
    /// Any excess over the required amount is returned to the sender.
    public entry fun post_job(
        board: &mut TrainingJobBoard,
        model_params_m: u64,
        dataset_tokens_m: u64,
        num_epochs: u32,
        batch_size: u32,
        sequence_length: u32,
        precision: u8,
        min_gpu_vram_gb: u32,
        dataset_blob_id: String,
        base_model_blob_id: String,
        deadline_ms: u64,
        payment: Coin<SUI>,
        clock: &Clock,
        ctx: &mut TxContext,
    ) {
        let now = clock::timestamp_ms(clock);

        // ── Parameter validation ───────────────────────────────────────────
        assert!(model_params_m > 0 && model_params_m <= MAX_MODEL_PARAMS_M,    E_INVALID_PARAMS);
        assert!(dataset_tokens_m > 0 && dataset_tokens_m <= MAX_DATASET_TOKENS_M, E_INVALID_PARAMS);
        assert!((num_epochs as u64) > 0 && (num_epochs as u64) <= MAX_EPOCHS,  E_INVALID_PARAMS);
        assert!(
            precision == PRECISION_FP32 || precision == PRECISION_FP16 ||
            precision == PRECISION_BF16 || precision == PRECISION_INT8,
            E_INVALID_PARAMS
        );
        assert!(
            deadline_ms >= now + MIN_OPEN_DURATION_MS &&
            deadline_ms <= now + MAX_OPEN_DURATION_MS,
            E_INVALID_PARAMS
        );

        // ── Pricing ────────────────────────────────────────────────────────
        let (miner_payout, protocol_fee) = compute_price(
            board, model_params_m, dataset_tokens_m, (num_epochs as u64), precision,
        );
        let total_required = miner_payout + protocol_fee;
        assert!(coin::value(&payment) >= total_required, E_INSUFFICIENT_PAYMENT);

        // ── Handle payment: split exact amount, return excess ──────────────
        let mut payment_mut = payment;
        let escrow_coin = if (coin::value(&payment_mut) == total_required) {
            payment_mut
        } else {
            let excess_amount = coin::value(&payment_mut) - total_required;
            let excess = coin::split(&mut payment_mut, excess_amount, ctx);
            transfer::public_transfer(excess, tx_context::sender(ctx));
            payment_mut
        };

        let customer = tx_context::sender(ctx);
        let job_id = board.job_counter;

        let job = TrainingJob {
            id: job_id,
            customer,
            model_params_m,
            dataset_tokens_m,
            num_epochs,
            batch_size,
            sequence_length,
            precision,
            min_gpu_vram_gb,
            dataset_blob_id,
            base_model_blob_id,
            escrow: coin::into_balance(escrow_coin),
            miner_payout,
            protocol_fee,
            posted_at_ms: now,
            deadline_ms,
            claimed_at_ms: 0,
            completed_at_ms: 0,
            status: STATUS_OPEN,
            miner_address: option::none(),
            miner_uid: 0,
            result_blob_id: std::string::utf8(b""),
            result_hash: vector::empty(),
        };

        table::add(&mut board.jobs, job_id, job);
        board.job_counter = board.job_counter + 1;

        event::emit(JobPosted {
            job_id,
            customer,
            miner_payout,
            protocol_fee,
            dataset_blob_id: table::borrow(&board.jobs, job_id).dataset_blob_id,
            deadline_ms,
        });
    }

    /// A miner claims an open job. Any address may claim (permissionless).
    /// `miner_uid` is 0 if the miner is not a registered peer.
    public entry fun claim_job(
        board: &mut TrainingJobBoard,
        job_id: u64,
        miner_uid: u64,
        clock: &Clock,
        ctx: &mut TxContext,
    ) {
        let now = clock::timestamp_ms(clock);
        let job = table::borrow_mut(&mut board.jobs, job_id);

        assert!(job.status == STATUS_OPEN, E_NOT_OPEN);
        assert!(now < job.deadline_ms, E_PAST_DEADLINE);

        let miner = tx_context::sender(ctx);
        job.status = STATUS_CLAIMED;
        job.miner_address = option::some(miner);
        job.miner_uid = miner_uid;
        job.claimed_at_ms = now;

        event::emit(JobClaimed {
            job_id,
            miner,
            miner_uid,
            deadline_ms: job.deadline_ms,
        });
    }

    /// The assigned miner submits the completed result.
    public entry fun complete_job(
        board: &mut TrainingJobBoard,
        job_id: u64,
        result_blob_id: String,
        result_hash: vector<u8>,
        clock: &Clock,
        ctx: &mut TxContext,
    ) {
        let now = clock::timestamp_ms(clock);
        let sender = tx_context::sender(ctx);
        let job = table::borrow_mut(&mut board.jobs, job_id);

        assert!(job.status == STATUS_CLAIMED, E_NOT_CLAIMED);
        assert!(
            option::is_some(&job.miner_address) &&
            *option::borrow(&job.miner_address) == sender,
            E_NOT_ASSIGNED_MINER
        );
        assert!(now <= job.deadline_ms, E_PAST_DEADLINE);

        job.status = STATUS_COMPLETED;
        job.result_blob_id = result_blob_id;
        job.result_hash = result_hash;
        job.completed_at_ms = now;

        event::emit(JobCompleted {
            job_id,
            miner: sender,
            result_blob_id: job.result_blob_id,
            completed_at_ms: now,
        });
    }

    /// Miner withdraws payment after the dispute window has passed.
    public entry fun withdraw_payment(
        board: &mut TrainingJobBoard,
        job_id: u64,
        clock: &Clock,
        ctx: &mut TxContext,
    ) {
        let now = clock::timestamp_ms(clock);
        let sender = tx_context::sender(ctx);

        {
            let job = table::borrow(&board.jobs, job_id);
            assert!(job.status == STATUS_COMPLETED, E_NOT_COMPLETED);
            assert!(
                option::is_some(&job.miner_address) &&
                *option::borrow(&job.miner_address) == sender,
                E_NOT_ASSIGNED_MINER
            );
            assert!(
                now > job.completed_at_ms + board.dispute_window_ms,
                E_DISPUTE_WINDOW_ACTIVE
            );
        };

        let miner_addr = *option::borrow(&table::borrow(&board.jobs, job_id).miner_address);
        let miner_payout = table::borrow(&board.jobs, job_id).miner_payout;

        let (miner_bal, fee_bal) = drain_job_for_settlement(board, job_id);
        balance::join(&mut board.fee_vault, fee_bal);
        transfer::public_transfer(coin::from_balance(miner_bal, ctx), miner_addr);

        event::emit(PaymentSettled {
            job_id,
            miner: miner_addr,
            amount: miner_payout,
        });
    }

    /// Customer disputes a completed result within the dispute window.
    public entry fun dispute_result(
        board: &mut TrainingJobBoard,
        job_id: u64,
        clock: &Clock,
        ctx: &mut TxContext,
    ) {
        let now = clock::timestamp_ms(clock);
        let sender = tx_context::sender(ctx);
        let job = table::borrow_mut(&mut board.jobs, job_id);

        assert!(job.status == STATUS_COMPLETED, E_NOT_COMPLETED);
        assert!(job.customer == sender, E_NOT_CUSTOMER);
        assert!(
            now <= job.completed_at_ms + board.dispute_window_ms,
            E_DISPUTE_WINDOW_PASSED
        );

        job.status = STATUS_DISPUTED;

        event::emit(JobDisputed { job_id, customer: sender });
    }

    /// Admin resolves a dispute. If pay_miner, miner receives miner_payout and
    /// protocol_fee goes to vault. Otherwise full escrow is returned to customer.
    public entry fun resolve_dispute(
        board: &mut TrainingJobBoard,
        _cap: &JobBoardAdminCap,
        job_id: u64,
        pay_miner: bool,
        ctx: &mut TxContext,
    ) {
        {
            let job = table::borrow(&board.jobs, job_id);
            assert!(job.status == STATUS_DISPUTED, E_NOT_DISPUTED);
        };

        if (pay_miner) {
            let miner_addr = *option::borrow(&table::borrow(&board.jobs, job_id).miner_address);
            let miner_payout = table::borrow(&board.jobs, job_id).miner_payout;
            let (miner_bal, fee_bal) = drain_job_for_settlement(board, job_id);
            balance::join(&mut board.fee_vault, fee_bal);
            transfer::public_transfer(coin::from_balance(miner_bal, ctx), miner_addr);
            event::emit(PaymentSettled {
                job_id,
                miner: miner_addr,
                amount: miner_payout,
            });
        } else {
            let customer = table::borrow(&board.jobs, job_id).customer;
            let refund_bal = drain_job_full_refund(board, job_id, STATUS_REFUNDED);
            let amount = balance::value(&refund_bal);
            transfer::public_transfer(coin::from_balance(refund_bal, ctx), customer);
            event::emit(JobRefunded { job_id, customer, amount });
        };

        event::emit(DisputeResolved { job_id, paid_miner: pay_miner });
    }

    /// Customer calls refund_job after the job deadline has passed (OPEN or CLAIMED).
    public entry fun refund_job(
        board: &mut TrainingJobBoard,
        job_id: u64,
        clock: &Clock,
        ctx: &mut TxContext,
    ) {
        let now = clock::timestamp_ms(clock);
        let sender = tx_context::sender(ctx);

        {
            let job = table::borrow(&board.jobs, job_id);
            assert!(job.status == STATUS_OPEN || job.status == STATUS_CLAIMED, E_NOT_OPEN);
            assert!(job.customer == sender, E_NOT_CUSTOMER);
            assert!(now > job.deadline_ms, E_DEADLINE_NOT_PASSED);
        };

        let refund_bal = drain_job_full_refund(board, job_id, STATUS_REFUNDED);
        let amount = balance::value(&refund_bal);
        transfer::public_transfer(coin::from_balance(refund_bal, ctx), sender);

        event::emit(JobRefunded { job_id, customer: sender, amount });
    }

    /// Customer cancels an unclaimed (STATUS_OPEN) job and recovers full escrow.
    public entry fun cancel_job(
        board: &mut TrainingJobBoard,
        job_id: u64,
        ctx: &mut TxContext,
    ) {
        let sender = tx_context::sender(ctx);

        {
            let job = table::borrow(&board.jobs, job_id);
            assert!(job.status == STATUS_OPEN, E_NOT_OPEN);
            assert!(job.customer == sender, E_NOT_CUSTOMER);
        };

        let cancel_bal = drain_job_full_refund(board, job_id, STATUS_REFUNDED);
        let amount = balance::value(&cancel_bal);
        transfer::public_transfer(coin::from_balance(cancel_bal, ctx), sender);

        event::emit(JobCancelled { job_id, customer: sender, amount });
    }

    /// Admin sweeps all accumulated protocol fees to the treasury address.
    public entry fun sweep_fees(
        board: &mut TrainingJobBoard,
        _cap: &JobBoardAdminCap,
        ctx: &mut TxContext,
    ) {
        let total = balance::value(&board.fee_vault);
        if (total > 0) {
            let sweep_bal = balance::split(&mut board.fee_vault, total);
            transfer::public_transfer(coin::from_balance(sweep_bal, ctx), board.treasury);
        };
    }

    // ── Governance ─────────────────────────────────────────────────────────────

    public entry fun update_price_per_unit(
        board: &mut TrainingJobBoard,
        _cap: &JobBoardAdminCap,
        new_price: u64,
    ) {
        assert!(new_price > 0, E_INVALID_PARAMS);
        board.price_per_unit = new_price;
    }

    /// Max 10% protocol fee.
    public entry fun update_fee_bps(
        board: &mut TrainingJobBoard,
        _cap: &JobBoardAdminCap,
        new_bps: u64,
    ) {
        assert!(new_bps <= 1_000, E_INVALID_PARAMS);
        board.protocol_fee_bps = new_bps;
    }

    /// Dispute window must be between 1 hour and 24 hours.
    public entry fun update_dispute_window(
        board: &mut TrainingJobBoard,
        _cap: &JobBoardAdminCap,
        new_ms: u64,
    ) {
        assert!(new_ms >= 3_600_000 && new_ms <= 86_400_000, E_INVALID_PARAMS);
        board.dispute_window_ms = new_ms;
    }

    public entry fun update_min_price(
        board: &mut TrainingJobBoard,
        _cap: &JobBoardAdminCap,
        new_price: u64,
    ) {
        assert!(new_price > 0, E_INVALID_PARAMS);
        board.min_price = new_price;
    }

    public entry fun update_treasury(
        board: &mut TrainingJobBoard,
        _cap: &JobBoardAdminCap,
        new_treasury: address,
    ) {
        board.treasury = new_treasury;
    }

    // ── View functions ─────────────────────────────────────────────────────────

    public fun job_count(board: &TrainingJobBoard): u64 {
        board.job_counter
    }

    public fun fee_vault_balance(board: &TrainingJobBoard): u64 {
        balance::value(&board.fee_vault)
    }

    public fun price_per_unit(board: &TrainingJobBoard): u64 {
        board.price_per_unit
    }

    public fun protocol_fee_bps(board: &TrainingJobBoard): u64 {
        board.protocol_fee_bps
    }

    public fun min_price(board: &TrainingJobBoard): u64 {
        board.min_price
    }

    public fun dispute_window_ms(board: &TrainingJobBoard): u64 {
        board.dispute_window_ms
    }

    public fun get_job_status(board: &TrainingJobBoard, job_id: u64): u8 {
        table::borrow(&board.jobs, job_id).status
    }

    public fun get_job_customer(board: &TrainingJobBoard, job_id: u64): address {
        table::borrow(&board.jobs, job_id).customer
    }

    public fun get_job_deadline(board: &TrainingJobBoard, job_id: u64): u64 {
        table::borrow(&board.jobs, job_id).deadline_ms
    }

    public fun get_job_miner_payout(board: &TrainingJobBoard, job_id: u64): u64 {
        table::borrow(&board.jobs, job_id).miner_payout
    }

    public fun get_job_result_blob_id(board: &TrainingJobBoard, job_id: u64): String {
        table::borrow(&board.jobs, job_id).result_blob_id
    }

    public fun get_job_dataset_blob_id(board: &TrainingJobBoard, job_id: u64): String {
        table::borrow(&board.jobs, job_id).dataset_blob_id
    }

    // ── Test-only status accessors ─────────────────────────────────────────────

    #[test_only]
    public fun status_open(): u8      { STATUS_OPEN }
    #[test_only]
    public fun status_claimed(): u8   { STATUS_CLAIMED }
    #[test_only]
    public fun status_completed(): u8 { STATUS_COMPLETED }
    #[test_only]
    public fun status_settled(): u8   { STATUS_SETTLED }
    #[test_only]
    public fun status_disputed(): u8  { STATUS_DISPUTED }
    #[test_only]
    public fun status_refunded(): u8  { STATUS_REFUNDED }

    #[test_only]
    public fun precision_int8(): u8   { PRECISION_INT8 }
    #[test_only]
    public fun precision_fp32(): u8   { PRECISION_FP32 }

    #[test_only]
    public fun default_price_per_unit(): u64    { DEFAULT_PRICE_PER_UNIT }
    #[test_only]
    public fun default_protocol_fee_bps(): u64  { DEFAULT_PROTOCOL_FEE_BPS }
    #[test_only]
    public fun default_min_price(): u64         { DEFAULT_MIN_PRICE }
    #[test_only]
    public fun default_dispute_window_ms(): u64 { DEFAULT_DISPUTE_WINDOW_MS }
    #[test_only]
    public fun min_open_duration_ms(): u64      { MIN_OPEN_DURATION_MS }
    #[test_only]
    public fun bps_denom(): u64                 { BPS_DENOM }

    #[test_only]
    public fun get_job_miner_address(board: &TrainingJobBoard, job_id: u64): Option<address> {
        table::borrow(&board.jobs, job_id).miner_address
    }

    #[test_only]
    public fun get_job_completed_at_ms(board: &TrainingJobBoard, job_id: u64): u64 {
        table::borrow(&board.jobs, job_id).completed_at_ms
    }

    #[test_only]
    public fun get_job_protocol_fee(board: &TrainingJobBoard, job_id: u64): u64 {
        table::borrow(&board.jobs, job_id).protocol_fee
    }
}
