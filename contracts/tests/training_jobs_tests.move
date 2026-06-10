// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

#[test_only]
module slcl::training_jobs_tests {
    use sui::test_scenario::{Self};
    use sui::coin::{Self, Coin};
    use sui::clock;
    use slcl::training_jobs::{Self, TrainingJobBoard, JobBoardAdminCap};
    use sui::sui::SUI;

    // ── Addresses ──────────────────────────────────────────────────────────────

    const ADMIN:    address = @0xA;
    const CUSTOMER: address = @0xB;
    const MINER:    address = @0xC;
    const MINER2:   address = @0xD;

    // ── Helpers ────────────────────────────────────────────────────────────────

    fun sui_coin(amount: u64, ctx: &mut sui::tx_context::TxContext): Coin<SUI> {
        coin::mint_for_testing<SUI>(amount, ctx)
    }

    /// Set up the board and return a clock starting at t=0.
    fun setup(): (test_scenario::Scenario, clock::Clock) {
        let mut s = test_scenario::begin(ADMIN);
        test_scenario::next_tx(&mut s, ADMIN);
        { training_jobs::init_for_testing(test_scenario::ctx(&mut s)); };
        let clk = clock::create_for_testing(test_scenario::ctx(&mut s));
        (s, clk)
    }

    /// Post a standard job (7_000_000M params × 10_000_000M tokens × 1 epoch).
    ///   compute_units = 7_000_000 × 10_000_000 × 1 / 1_000 = 70_000_000_000_000
    ///   raw_price     = 70_000_000_000_000 × 10_000 = 700_000_000_000_000_000
    ///                   (exceeds u64; use model_params_m=7_000 for safe arithmetic)
    ///
    /// Practical standard job: model_params_m=7_000, dataset_tokens_m=10_000
    ///   compute_units = 7_000 × 10_000 × 1 / 1_000 = 70_000
    ///   raw_price     = 70_000 × 10_000 = 700_000_000
    ///   miner_payout  = max(700_000_000, 1_000_000_000) = 1_000_000_000  (min_price floor)
    ///   protocol_fee  = 1_000_000_000 × 500 / 10_000 = 50_000_000
    ///   total         = 1_050_000_000
    ///
    /// Caller must already be in a next_tx block as CUSTOMER.
    fun post_standard_job(
        board: &mut TrainingJobBoard,
        clk: &clock::Clock,
        ctx: &mut sui::tx_context::TxContext,
    ): u64 {
        let now = clock::timestamp_ms(clk);
        let deadline = now + training_jobs::min_open_duration_ms() + 1;
        let (miner_payout, protocol_fee) = training_jobs::compute_price(
            board, 7_000, 10_000, 1, training_jobs::precision_fp32(),
        );
        let total = miner_payout + protocol_fee;
        training_jobs::post_job(
            board,
            7_000,   // model_params_m  (7000M = 7B params)
            10_000,  // dataset_tokens_m (10000M = 10B tokens)
            1,       // num_epochs
            32,      // batch_size
            512,     // sequence_length
            training_jobs::precision_fp32(),
            80,      // min_gpu_vram_gb
            std::string::utf8(b"walrus:abc123"),
            std::string::utf8(b""),
            deadline,
            sui_coin(total, ctx),
            clk,
            ctx,
        );
        0 // job_id
    }

    // ── Test 1: compute_price for 7B model × 10B tokens × 1 epoch ─────────────
    //
    // model_params_m=7_000, dataset_tokens_m=10_000, num_epochs=1, FP32
    // compute_units = 7_000 × 10_000 × 1 / 1_000 = 70_000
    // raw_price     = 70_000 × 10_000 = 700_000_000
    // miner_payout  = max(700_000_000, 1_000_000_000) = 1_000_000_000  (min_price floor)
    // protocol_fee  = 1_000_000_000 × 500 / 10_000 = 50_000_000

    #[test]
    fun test_compute_price_7b_10b() {
        let (mut s, clk) = setup();
        test_scenario::next_tx(&mut s, CUSTOMER);
        {
            let board: TrainingJobBoard = test_scenario::take_shared(&s);
            let (miner_payout, protocol_fee) = training_jobs::compute_price(
                &board, 7_000, 10_000, 1, training_jobs::precision_fp32(),
            );
            // min_price floor kicks in: 700_000_000 < 1_000_000_000
            assert!(miner_payout == 1_000_000_000, 0);
            assert!(protocol_fee == 50_000_000, 1);
            test_scenario::return_shared(board);
        };
        clock::destroy_for_testing(clk);
        test_scenario::end(s);
    }

    // ── Test 1b: large job that clears the min_price floor ────────────────────
    //
    // model_params_m=700_000, dataset_tokens_m=1_000_000, num_epochs=1, FP32
    // Step-by-step (Move evaluates left to right):
    //   700_000 × 1_000_000 × 1 / 1_000
    //   = 700_000_000_000 × 1 / 1_000
    //   = 700_000_000_000 / 1_000
    //   = 700_000_000  (compute_units)
    // raw_price    = 700_000_000 × 10_000 = 7_000_000_000_000
    // miner_payout = max(7_000_000_000_000, 1_000_000_000) = 7_000_000_000_000
    // protocol_fee = 7_000_000_000_000 × 500 / 10_000 = 350_000_000_000

    #[test]
    fun test_compute_price_large_job_above_floor() {
        let (mut s, clk) = setup();
        test_scenario::next_tx(&mut s, CUSTOMER);
        {
            let board: TrainingJobBoard = test_scenario::take_shared(&s);
            let (miner_payout, protocol_fee) = training_jobs::compute_price(
                &board, 700_000, 1_000_000, 1, training_jobs::precision_fp32(),
            );
            assert!(miner_payout == 7_000_000_000_000, 0);
            assert!(protocol_fee == 350_000_000_000, 1);
            test_scenario::return_shared(board);
        };
        clock::destroy_for_testing(clk);
        test_scenario::end(s);
    }

    // ── Test 2: INT8 precision halves compute units ────────────────────────────
    //
    // Use a job large enough to escape the min_price floor for both precisions.
    // model_params_m=700_000, dataset_tokens_m=1_000_000, num_epochs=1
    //   FP32: compute_units=700_000_000 → miner_payout = 7_000_000_000_000
    //   INT8: compute_units=350_000_000 → miner_payout = 3_500_000_000_000

    #[test]
    fun test_compute_price_int8_half() {
        let (mut s, clk) = setup();
        test_scenario::next_tx(&mut s, CUSTOMER);
        {
            let board: TrainingJobBoard = test_scenario::take_shared(&s);
            let (miner_payout_fp32, _) = training_jobs::compute_price(
                &board, 700_000, 1_000_000, 1, training_jobs::precision_fp32(),
            );
            let (miner_payout_int8, protocol_fee_int8) = training_jobs::compute_price(
                &board, 700_000, 1_000_000, 1, training_jobs::precision_int8(),
            );
            assert!(miner_payout_int8 == miner_payout_fp32 / 2, 0);
            assert!(miner_payout_int8 == 3_500_000_000_000, 1);
            // fee = 3_500_000_000_000 × 500 / 10_000 = 175_000_000_000
            assert!(protocol_fee_int8 == 175_000_000_000, 2);
            test_scenario::return_shared(board);
        };
        clock::destroy_for_testing(clk);
        test_scenario::end(s);
    }

    // ── Test 3: min_price floor when compute_units rounds to 0 ────────────────

    #[test]
    fun test_compute_price_min_price_floor() {
        let (mut s, clk) = setup();
        test_scenario::next_tx(&mut s, CUSTOMER);
        {
            let board: TrainingJobBoard = test_scenario::take_shared(&s);
            // 1 × 1 × 1 / 1_000 = 0 compute units → raw_price = 0 → floor to min_price
            let (miner_payout, _) = training_jobs::compute_price(
                &board, 1, 1, 1, training_jobs::precision_fp32(),
            );
            assert!(miner_payout == training_jobs::default_min_price(), 0);
            test_scenario::return_shared(board);
        };
        clock::destroy_for_testing(clk);
        test_scenario::end(s);
    }

    // ── Test 4: post_job creates job and increments counter ───────────────────

    #[test]
    fun test_post_job_emits_and_creates() {
        let (mut s, mut clk) = setup();
        test_scenario::next_tx(&mut s, CUSTOMER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            assert!(training_jobs::job_count(&board) == 0, 0);
            post_standard_job(&mut board, &clk, test_scenario::ctx(&mut s));
            assert!(training_jobs::job_count(&board) == 1, 1);
            let (mp, _pf) = training_jobs::compute_price(
                &board, 7_000, 10_000, 1, training_jobs::precision_fp32(),
            );
            assert!(training_jobs::get_job_miner_payout(&board, 0) == mp, 2);
            assert!(training_jobs::get_job_status(&board, 0) == training_jobs::status_open(), 3);
            test_scenario::return_shared(board);
        };
        clock::destroy_for_testing(clk);
        test_scenario::end(s);
    }

    // ── Test 5: excess payment is returned to sender ──────────────────────────

    #[test]
    fun test_post_job_returns_excess() {
        let (mut s, mut clk) = setup();
        test_scenario::next_tx(&mut s, CUSTOMER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            let (miner_payout, protocol_fee) = training_jobs::compute_price(
                &board, 7_000, 10_000, 1, training_jobs::precision_fp32(),
            );
            let total_required = miner_payout + protocol_fee;
            let overpay = 999_999_999;
            let now = clock::timestamp_ms(&clk);
            let deadline = now + training_jobs::min_open_duration_ms() + 1;
            training_jobs::post_job(
                &mut board,
                7_000, 10_000, 1, 32, 512,
                training_jobs::precision_fp32(),
                80,
                std::string::utf8(b"walrus:abc123"),
                std::string::utf8(b""),
                deadline,
                sui_coin(total_required + overpay, test_scenario::ctx(&mut s)),
                &clk,
                test_scenario::ctx(&mut s),
            );
            test_scenario::return_shared(board);
        };
        // Verify excess coin was transferred back to CUSTOMER
        test_scenario::next_tx(&mut s, CUSTOMER);
        {
            let excess_coin: Coin<SUI> = test_scenario::take_from_sender(&s);
            assert!(coin::value(&excess_coin) == 999_999_999, 0);
            test_scenario::return_to_sender(&s, excess_coin);
        };
        clock::destroy_for_testing(clk);
        test_scenario::end(s);
    }

    // ── Test 6: claim_job transitions status to CLAIMED ───────────────────────

    #[test]
    fun test_claim_job_basic() {
        let (mut s, mut clk) = setup();

        test_scenario::next_tx(&mut s, CUSTOMER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            post_standard_job(&mut board, &clk, test_scenario::ctx(&mut s));
            test_scenario::return_shared(board);
        };

        test_scenario::next_tx(&mut s, MINER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            training_jobs::claim_job(&mut board, 0, 42, &clk, test_scenario::ctx(&mut s));
            assert!(training_jobs::get_job_status(&board, 0) == training_jobs::status_claimed(), 0);
            let miner_opt = training_jobs::get_job_miner_address(&board, 0);
            assert!(std::option::is_some(&miner_opt), 1);
            assert!(*std::option::borrow(&miner_opt) == MINER, 2);
            test_scenario::return_shared(board);
        };

        clock::destroy_for_testing(clk);
        test_scenario::end(s);
    }

    // ── Test 7: full happy path ────────────────────────────────────────────────

    #[test]
    fun test_full_happy_path() {
        let (mut s, mut clk) = setup();

        // Post
        test_scenario::next_tx(&mut s, CUSTOMER);
        let expected_payout = {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            let (mp, pf) = training_jobs::compute_price(
                &board, 7_000, 10_000, 1, training_jobs::precision_fp32(),
            );
            post_standard_job(&mut board, &clk, test_scenario::ctx(&mut s));
            test_scenario::return_shared(board);
            mp
        };

        // Claim
        test_scenario::next_tx(&mut s, MINER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            training_jobs::claim_job(&mut board, 0, 0, &clk, test_scenario::ctx(&mut s));
            test_scenario::return_shared(board);
        };

        // Complete
        test_scenario::next_tx(&mut s, MINER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            training_jobs::complete_job(
                &mut board, 0,
                std::string::utf8(b"walrus:result456"),
                b"sha256hashbytes",
                &clk,
                test_scenario::ctx(&mut s),
            );
            assert!(training_jobs::get_job_status(&board, 0) == training_jobs::status_completed(), 0);
            test_scenario::return_shared(board);
        };

        // Advance clock past dispute window
        let completed_at = {
            test_scenario::next_tx(&mut s, MINER);
            let board: TrainingJobBoard = test_scenario::take_shared(&s);
            let t = training_jobs::get_job_completed_at_ms(&board, 0);
            test_scenario::return_shared(board);
            t
        };
        clock::set_for_testing(
            &mut clk,
            completed_at + training_jobs::default_dispute_window_ms() + 1,
        );

        // Withdraw
        test_scenario::next_tx(&mut s, MINER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            let expected_fee = training_jobs::get_job_protocol_fee(&board, 0);
            training_jobs::withdraw_payment(&mut board, 0, &clk, test_scenario::ctx(&mut s));
            assert!(training_jobs::get_job_status(&board, 0) == training_jobs::status_settled(), 1);
            assert!(training_jobs::fee_vault_balance(&board) == expected_fee, 2);
            test_scenario::return_shared(board);
        };

        // Verify miner received coin
        test_scenario::next_tx(&mut s, MINER);
        {
            let pay: Coin<SUI> = test_scenario::take_from_sender(&s);
            assert!(coin::value(&pay) == expected_payout, 0);
            test_scenario::return_to_sender(&s, pay);
        };

        clock::destroy_for_testing(clk);
        test_scenario::end(s);
    }

    // ── Test 8: dispute resolved in miner's favour ────────────────────────────

    #[test]
    fun test_dispute_resolved_for_miner() {
        let (mut s, mut clk) = setup();

        // Post and capture expected payout
        test_scenario::next_tx(&mut s, CUSTOMER);
        let expected_payout = {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            let (mp, _) = training_jobs::compute_price(
                &board, 7_000, 10_000, 1, training_jobs::precision_fp32(),
            );
            post_standard_job(&mut board, &clk, test_scenario::ctx(&mut s));
            test_scenario::return_shared(board);
            mp
        };

        test_scenario::next_tx(&mut s, MINER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            training_jobs::claim_job(&mut board, 0, 0, &clk, test_scenario::ctx(&mut s));
            test_scenario::return_shared(board);
        };

        test_scenario::next_tx(&mut s, MINER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            training_jobs::complete_job(
                &mut board, 0,
                std::string::utf8(b"walrus:result"),
                b"hash",
                &clk,
                test_scenario::ctx(&mut s),
            );
            test_scenario::return_shared(board);
        };

        // Customer disputes within window
        test_scenario::next_tx(&mut s, CUSTOMER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            training_jobs::dispute_result(&mut board, 0, &clk, test_scenario::ctx(&mut s));
            assert!(training_jobs::get_job_status(&board, 0) == training_jobs::status_disputed(), 0);
            test_scenario::return_shared(board);
        };

        // Admin resolves: pay miner
        test_scenario::next_tx(&mut s, ADMIN);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            let cap: JobBoardAdminCap = test_scenario::take_from_sender(&s);
            training_jobs::resolve_dispute(&mut board, &cap, 0, true, test_scenario::ctx(&mut s));
            assert!(training_jobs::get_job_status(&board, 0) == training_jobs::status_settled(), 1);
            test_scenario::return_to_sender(&s, cap);
            test_scenario::return_shared(board);
        };

        // Miner should have received payment
        test_scenario::next_tx(&mut s, MINER);
        {
            let pay: Coin<SUI> = test_scenario::take_from_sender(&s);
            assert!(coin::value(&pay) == expected_payout, 0);
            test_scenario::return_to_sender(&s, pay);
        };

        clock::destroy_for_testing(clk);
        test_scenario::end(s);
    }

    // ── Test 9: dispute resolved in customer's favour ─────────────────────────

    #[test]
    fun test_dispute_resolved_for_customer() {
        let (mut s, mut clk) = setup();

        // Post and capture total locked in escrow
        test_scenario::next_tx(&mut s, CUSTOMER);
        let expected_refund = {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            let (mp, pf) = training_jobs::compute_price(
                &board, 7_000, 10_000, 1, training_jobs::precision_fp32(),
            );
            post_standard_job(&mut board, &clk, test_scenario::ctx(&mut s));
            test_scenario::return_shared(board);
            mp + pf
        };

        test_scenario::next_tx(&mut s, MINER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            training_jobs::claim_job(&mut board, 0, 0, &clk, test_scenario::ctx(&mut s));
            test_scenario::return_shared(board);
        };

        test_scenario::next_tx(&mut s, MINER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            training_jobs::complete_job(
                &mut board, 0,
                std::string::utf8(b"walrus:result"),
                b"hash",
                &clk,
                test_scenario::ctx(&mut s),
            );
            test_scenario::return_shared(board);
        };

        test_scenario::next_tx(&mut s, CUSTOMER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            training_jobs::dispute_result(&mut board, 0, &clk, test_scenario::ctx(&mut s));
            test_scenario::return_shared(board);
        };

        // Admin resolves: refund customer
        test_scenario::next_tx(&mut s, ADMIN);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            let cap: JobBoardAdminCap = test_scenario::take_from_sender(&s);
            training_jobs::resolve_dispute(&mut board, &cap, 0, false, test_scenario::ctx(&mut s));
            assert!(training_jobs::get_job_status(&board, 0) == training_jobs::status_refunded(), 0);
            // fee vault should remain 0 (full refund path)
            assert!(training_jobs::fee_vault_balance(&board) == 0, 1);
            test_scenario::return_to_sender(&s, cap);
            test_scenario::return_shared(board);
        };

        // Customer gets full escrow back (miner_payout + protocol_fee)
        test_scenario::next_tx(&mut s, CUSTOMER);
        {
            let refund: Coin<SUI> = test_scenario::take_from_sender(&s);
            assert!(coin::value(&refund) == expected_refund, 0);
            test_scenario::return_to_sender(&s, refund);
        };

        clock::destroy_for_testing(clk);
        test_scenario::end(s);
    }

    // ── Test 10: refund unclaimed (OPEN) job after deadline ───────────────────

    #[test]
    fun test_refund_unclaimed_expired() {
        let (mut s, mut clk) = setup();

        // Post and capture total
        test_scenario::next_tx(&mut s, CUSTOMER);
        let expected_refund = {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            let (mp, pf) = training_jobs::compute_price(
                &board, 7_000, 10_000, 1, training_jobs::precision_fp32(),
            );
            post_standard_job(&mut board, &clk, test_scenario::ctx(&mut s));
            test_scenario::return_shared(board);
            mp + pf
        };

        // Advance past deadline
        let deadline = {
            test_scenario::next_tx(&mut s, CUSTOMER);
            let board: TrainingJobBoard = test_scenario::take_shared(&s);
            let d = training_jobs::get_job_deadline(&board, 0);
            test_scenario::return_shared(board);
            d
        };
        clock::set_for_testing(&mut clk, deadline + 1);

        test_scenario::next_tx(&mut s, CUSTOMER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            training_jobs::refund_job(&mut board, 0, &clk, test_scenario::ctx(&mut s));
            assert!(training_jobs::get_job_status(&board, 0) == training_jobs::status_refunded(), 0);
            test_scenario::return_shared(board);
        };

        test_scenario::next_tx(&mut s, CUSTOMER);
        {
            let refund: Coin<SUI> = test_scenario::take_from_sender(&s);
            assert!(coin::value(&refund) == expected_refund, 0);
            test_scenario::return_to_sender(&s, refund);
        };

        clock::destroy_for_testing(clk);
        test_scenario::end(s);
    }

    // ── Test 11: refund claimed-but-abandoned job after deadline ──────────────

    #[test]
    fun test_refund_claimed_abandoned() {
        let (mut s, mut clk) = setup();

        // Post and capture total
        test_scenario::next_tx(&mut s, CUSTOMER);
        let expected_refund = {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            let (mp, pf) = training_jobs::compute_price(
                &board, 7_000, 10_000, 1, training_jobs::precision_fp32(),
            );
            post_standard_job(&mut board, &clk, test_scenario::ctx(&mut s));
            test_scenario::return_shared(board);
            mp + pf
        };

        test_scenario::next_tx(&mut s, MINER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            training_jobs::claim_job(&mut board, 0, 0, &clk, test_scenario::ctx(&mut s));
            test_scenario::return_shared(board);
        };

        // Advance past deadline without completing
        let deadline = {
            test_scenario::next_tx(&mut s, CUSTOMER);
            let board: TrainingJobBoard = test_scenario::take_shared(&s);
            let d = training_jobs::get_job_deadline(&board, 0);
            test_scenario::return_shared(board);
            d
        };
        clock::set_for_testing(&mut clk, deadline + 1);

        test_scenario::next_tx(&mut s, CUSTOMER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            training_jobs::refund_job(&mut board, 0, &clk, test_scenario::ctx(&mut s));
            assert!(training_jobs::get_job_status(&board, 0) == training_jobs::status_refunded(), 0);
            test_scenario::return_shared(board);
        };

        test_scenario::next_tx(&mut s, CUSTOMER);
        {
            let refund: Coin<SUI> = test_scenario::take_from_sender(&s);
            assert!(coin::value(&refund) == expected_refund, 0);
            test_scenario::return_to_sender(&s, refund);
        };

        clock::destroy_for_testing(clk);
        test_scenario::end(s);
    }

    // ── Test 12: customer cancels unclaimed job ────────────────────────────────

    #[test]
    fun test_cancel_unclaimed() {
        let (mut s, mut clk) = setup();

        // Post and capture total
        test_scenario::next_tx(&mut s, CUSTOMER);
        let expected_refund = {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            let (mp, pf) = training_jobs::compute_price(
                &board, 7_000, 10_000, 1, training_jobs::precision_fp32(),
            );
            post_standard_job(&mut board, &clk, test_scenario::ctx(&mut s));
            test_scenario::return_shared(board);
            mp + pf
        };

        test_scenario::next_tx(&mut s, CUSTOMER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            training_jobs::cancel_job(&mut board, 0, test_scenario::ctx(&mut s));
            assert!(training_jobs::get_job_status(&board, 0) == training_jobs::status_refunded(), 0);
            test_scenario::return_shared(board);
        };

        test_scenario::next_tx(&mut s, CUSTOMER);
        {
            let refund: Coin<SUI> = test_scenario::take_from_sender(&s);
            assert!(coin::value(&refund) == expected_refund, 0);
            test_scenario::return_to_sender(&s, refund);
        };

        clock::destroy_for_testing(clk);
        test_scenario::end(s);
    }

    // ── Test 13: admin sweeps accumulated fees ─────────────────────────────────

    #[test]
    fun test_sweep_fees() {
        let (mut s, mut clk) = setup();

        // Post → Claim → Complete → wait → Withdraw
        test_scenario::next_tx(&mut s, CUSTOMER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            post_standard_job(&mut board, &clk, test_scenario::ctx(&mut s));
            test_scenario::return_shared(board);
        };

        test_scenario::next_tx(&mut s, MINER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            training_jobs::claim_job(&mut board, 0, 0, &clk, test_scenario::ctx(&mut s));
            test_scenario::return_shared(board);
        };

        test_scenario::next_tx(&mut s, MINER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            training_jobs::complete_job(
                &mut board, 0,
                std::string::utf8(b"walrus:result"),
                b"hash",
                &clk,
                test_scenario::ctx(&mut s),
            );
            test_scenario::return_shared(board);
        };

        let completed_at = {
            test_scenario::next_tx(&mut s, MINER);
            let board: TrainingJobBoard = test_scenario::take_shared(&s);
            let t = training_jobs::get_job_completed_at_ms(&board, 0);
            test_scenario::return_shared(board);
            t
        };
        clock::set_for_testing(
            &mut clk,
            completed_at + training_jobs::default_dispute_window_ms() + 1,
        );

        // Capture expected fee before withdrawing
        let expected_fee = {
            test_scenario::next_tx(&mut s, MINER);
            let board: TrainingJobBoard = test_scenario::take_shared(&s);
            let f = training_jobs::get_job_protocol_fee(&board, 0);
            test_scenario::return_shared(board);
            f
        };

        test_scenario::next_tx(&mut s, MINER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            training_jobs::withdraw_payment(&mut board, 0, &clk, test_scenario::ctx(&mut s));
            assert!(training_jobs::fee_vault_balance(&board) == expected_fee, 0);
            test_scenario::return_shared(board);
        };

        // Admin sweeps
        test_scenario::next_tx(&mut s, ADMIN);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            let cap: JobBoardAdminCap = test_scenario::take_from_sender(&s);
            training_jobs::sweep_fees(&mut board, &cap, test_scenario::ctx(&mut s));
            assert!(training_jobs::fee_vault_balance(&board) == 0, 0);
            test_scenario::return_to_sender(&s, cap);
            test_scenario::return_shared(board);
        };

        // Treasury (ADMIN) received fees
        test_scenario::next_tx(&mut s, ADMIN);
        {
            let fees: Coin<SUI> = test_scenario::take_from_sender(&s);
            assert!(coin::value(&fees) == expected_fee, 0);
            test_scenario::return_to_sender(&s, fees);
        };

        clock::destroy_for_testing(clk);
        test_scenario::end(s);
    }

    // ── Test 14: claiming past deadline aborts with E_PAST_DEADLINE (7) ───────

    #[test]
    #[expected_failure(abort_code = 7, location = slcl::training_jobs)]
    fun test_claim_past_deadline_aborts() {
        let (mut s, mut clk) = setup();

        test_scenario::next_tx(&mut s, CUSTOMER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            post_standard_job(&mut board, &clk, test_scenario::ctx(&mut s));
            test_scenario::return_shared(board);
        };

        let deadline = {
            test_scenario::next_tx(&mut s, MINER);
            let board: TrainingJobBoard = test_scenario::take_shared(&s);
            let d = training_jobs::get_job_deadline(&board, 0);
            test_scenario::return_shared(board);
            d
        };
        clock::set_for_testing(&mut clk, deadline + 1);

        test_scenario::next_tx(&mut s, MINER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            training_jobs::claim_job(&mut board, 0, 0, &clk, test_scenario::ctx(&mut s));
            test_scenario::return_shared(board);
        };

        clock::destroy_for_testing(clk);
        test_scenario::end(s);
    }

    // ── Test 15: second claim aborts with E_NOT_OPEN (3) ─────────────────────

    #[test]
    #[expected_failure(abort_code = 3, location = slcl::training_jobs)]
    fun test_duplicate_claim_aborts() {
        let (mut s, mut clk) = setup();

        test_scenario::next_tx(&mut s, CUSTOMER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            post_standard_job(&mut board, &clk, test_scenario::ctx(&mut s));
            test_scenario::return_shared(board);
        };

        test_scenario::next_tx(&mut s, MINER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            training_jobs::claim_job(&mut board, 0, 0, &clk, test_scenario::ctx(&mut s));
            test_scenario::return_shared(board);
        };

        // Second miner tries to claim same job
        test_scenario::next_tx(&mut s, MINER2);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            training_jobs::claim_job(&mut board, 0, 0, &clk, test_scenario::ctx(&mut s));
            test_scenario::return_shared(board);
        };

        clock::destroy_for_testing(clk);
        test_scenario::end(s);
    }

    // ── Test 16: wrong miner calling complete_job aborts (E_NOT_ASSIGNED_MINER=5) ──

    #[test]
    #[expected_failure(abort_code = 5, location = slcl::training_jobs)]
    fun test_wrong_miner_complete_aborts() {
        let (mut s, mut clk) = setup();

        test_scenario::next_tx(&mut s, CUSTOMER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            post_standard_job(&mut board, &clk, test_scenario::ctx(&mut s));
            test_scenario::return_shared(board);
        };

        test_scenario::next_tx(&mut s, MINER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            training_jobs::claim_job(&mut board, 0, 0, &clk, test_scenario::ctx(&mut s));
            test_scenario::return_shared(board);
        };

        // MINER2 tries to complete the job
        test_scenario::next_tx(&mut s, MINER2);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            training_jobs::complete_job(
                &mut board, 0,
                std::string::utf8(b"walrus:fake"),
                b"fakehash",
                &clk,
                test_scenario::ctx(&mut s),
            );
            test_scenario::return_shared(board);
        };

        clock::destroy_for_testing(clk);
        test_scenario::end(s);
    }

    // ── Test 17: withdraw inside dispute window aborts (E_DISPUTE_WINDOW_ACTIVE=8) ──

    #[test]
    #[expected_failure(abort_code = 8, location = slcl::training_jobs)]
    fun test_withdraw_in_dispute_window_aborts() {
        let (mut s, mut clk) = setup();

        test_scenario::next_tx(&mut s, CUSTOMER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            post_standard_job(&mut board, &clk, test_scenario::ctx(&mut s));
            test_scenario::return_shared(board);
        };

        test_scenario::next_tx(&mut s, MINER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            training_jobs::claim_job(&mut board, 0, 0, &clk, test_scenario::ctx(&mut s));
            test_scenario::return_shared(board);
        };

        test_scenario::next_tx(&mut s, MINER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            training_jobs::complete_job(
                &mut board, 0,
                std::string::utf8(b"walrus:result"),
                b"hash",
                &clk,
                test_scenario::ctx(&mut s),
            );
            test_scenario::return_shared(board);
        };

        // Do NOT advance clock past dispute window — attempt immediate withdrawal
        test_scenario::next_tx(&mut s, MINER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            training_jobs::withdraw_payment(&mut board, 0, &clk, test_scenario::ctx(&mut s));
            test_scenario::return_shared(board);
        };

        clock::destroy_for_testing(clk);
        test_scenario::end(s);
    }

    // ── Test 18: dispute after window aborts (E_DISPUTE_WINDOW_PASSED=9) ────────

    #[test]
    #[expected_failure(abort_code = 9, location = slcl::training_jobs)]
    fun test_dispute_after_window_aborts() {
        let (mut s, mut clk) = setup();

        test_scenario::next_tx(&mut s, CUSTOMER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            post_standard_job(&mut board, &clk, test_scenario::ctx(&mut s));
            test_scenario::return_shared(board);
        };

        test_scenario::next_tx(&mut s, MINER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            training_jobs::claim_job(&mut board, 0, 0, &clk, test_scenario::ctx(&mut s));
            test_scenario::return_shared(board);
        };

        test_scenario::next_tx(&mut s, MINER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            training_jobs::complete_job(
                &mut board, 0,
                std::string::utf8(b"walrus:result"),
                b"hash",
                &clk,
                test_scenario::ctx(&mut s),
            );
            test_scenario::return_shared(board);
        };

        // Advance clock past dispute window
        let completed_at = {
            test_scenario::next_tx(&mut s, CUSTOMER);
            let board: TrainingJobBoard = test_scenario::take_shared(&s);
            let t = training_jobs::get_job_completed_at_ms(&board, 0);
            test_scenario::return_shared(board);
            t
        };
        clock::set_for_testing(
            &mut clk,
            completed_at + training_jobs::default_dispute_window_ms() + 1,
        );

        // Customer tries to dispute after window
        test_scenario::next_tx(&mut s, CUSTOMER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            training_jobs::dispute_result(&mut board, 0, &clk, test_scenario::ctx(&mut s));
            test_scenario::return_shared(board);
        };

        clock::destroy_for_testing(clk);
        test_scenario::end(s);
    }

    // ── Test 19: refund_job before deadline aborts (E_DEADLINE_NOT_PASSED=6) ──

    #[test]
    #[expected_failure(abort_code = 6, location = slcl::training_jobs)]
    fun test_refund_before_deadline_aborts() {
        let (mut s, mut clk) = setup();

        test_scenario::next_tx(&mut s, CUSTOMER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            post_standard_job(&mut board, &clk, test_scenario::ctx(&mut s));
            test_scenario::return_shared(board);
        };

        // Attempt refund immediately (deadline not yet passed)
        test_scenario::next_tx(&mut s, CUSTOMER);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            training_jobs::refund_job(&mut board, 0, &clk, test_scenario::ctx(&mut s));
            test_scenario::return_shared(board);
        };

        clock::destroy_for_testing(clk);
        test_scenario::end(s);
    }

    // ── Test 20: admin updates price_per_unit and new price applies ───────────

    #[test]
    fun test_governance_update_price() {
        let (mut s, mut clk) = setup();

        // Update price
        test_scenario::next_tx(&mut s, ADMIN);
        {
            let mut board: TrainingJobBoard = test_scenario::take_shared(&s);
            let cap: JobBoardAdminCap = test_scenario::take_from_sender(&s);
            training_jobs::update_price_per_unit(&mut board, &cap, 20_000);
            assert!(training_jobs::price_per_unit(&board) == 20_000, 0);
            test_scenario::return_to_sender(&s, cap);
            test_scenario::return_shared(board);
        };

        // Verify new price applies to compute_price
        // compute_units = 7_000 × 10_000 × 1 / 1_000 = 70_000
        // raw_price = 70_000 × 20_000 = 1_400_000_000
        // miner_payout = max(1_400_000_000, 1_000_000_000) = 1_400_000_000
        test_scenario::next_tx(&mut s, CUSTOMER);
        {
            let board: TrainingJobBoard = test_scenario::take_shared(&s);
            let (miner_payout, _) = training_jobs::compute_price(
                &board, 7_000, 10_000, 1, training_jobs::precision_fp32(),
            );
            assert!(miner_payout == 1_400_000_000, 0);
            test_scenario::return_shared(board);
        };

        clock::destroy_for_testing(clk);
        test_scenario::end(s);
    }
}
