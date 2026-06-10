// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

#[test_only]
module slcl::gradient_registry_tests {
    use sui::test_scenario;
    use sui::clock;
    use slcl::gradient_registry::{
        Self, GradientRegistry,
    };

    // ── helpers ───────────────────────────────────────────────────────────────

    fun walrus_ref(): vector<u8> { b"walrus:abc123def456" }
    fun r2_ref(): vector<u8>     { b"gradient-42-7-v1.pt" }
    fun content_hash(): vector<u8> {
        b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    }

    // ── tests ─────────────────────────────────────────────────────────────────

    #[test]
    fun test_submit_creates_window_lazily() {
        let miner = @0xA;
        let mut s = test_scenario::begin(miner);

        test_scenario::next_tx(&mut s, miner);
        { gradient_registry::init_for_testing(test_scenario::ctx(&mut s)); };

        test_scenario::next_tx(&mut s, miner);
        {
            let mut reg: GradientRegistry = test_scenario::take_shared(&s);
            let clk = clock::create_for_testing(test_scenario::ctx(&mut s));

            assert!(gradient_registry::submission_count(&reg, 42) == 0, 0);

            gradient_registry::submit_gradient(
                &mut reg, 42, 7, walrus_ref(), content_hash(), 1,
                &clk, test_scenario::ctx(&mut s),
            );

            assert!(gradient_registry::submission_count(&reg, 42) == 1, 1);

            clock::destroy_for_testing(clk);
            test_scenario::return_shared(reg);
        };

        test_scenario::end(s);
    }

    #[test]
    fun test_list_submissions_returns_correct_fields() {
        let miner = @0xA;
        let mut s = test_scenario::begin(miner);

        test_scenario::next_tx(&mut s, miner);
        { gradient_registry::init_for_testing(test_scenario::ctx(&mut s)); };

        test_scenario::next_tx(&mut s, miner);
        {
            let mut reg: GradientRegistry = test_scenario::take_shared(&s);
            let clk = clock::create_for_testing(test_scenario::ctx(&mut s));

            gradient_registry::submit_gradient(
                &mut reg, 10, 7, walrus_ref(), content_hash(), 1,
                &clk, test_scenario::ctx(&mut s),
            );

            let subs = gradient_registry::list_submissions(&reg, 10);
            assert!(vector::length(&subs) == 1, 0);

            let sub = vector::borrow(&subs, 0);
            assert!(gradient_registry::sub_miner_uid(sub) == 7, 1);
            assert!(gradient_registry::sub_storage_ref(sub) == walrus_ref(), 2);
            assert!(gradient_registry::sub_content_hash(sub) == content_hash(), 3);
            assert!(gradient_registry::sub_encryption_kind(sub) == 1, 4);
            assert!(gradient_registry::sub_submitter(sub) == miner, 5);

            clock::destroy_for_testing(clk);
            test_scenario::return_shared(reg);
        };

        test_scenario::end(s);
    }

    #[test]
    fun test_multiple_miners_same_window() {
        let admin  = @0xA;
        let miner1 = @0xB;
        let miner2 = @0xC;
        let mut s = test_scenario::begin(admin);

        test_scenario::next_tx(&mut s, admin);
        { gradient_registry::init_for_testing(test_scenario::ctx(&mut s)); };

        test_scenario::next_tx(&mut s, miner1);
        {
            let mut reg: GradientRegistry = test_scenario::take_shared(&s);
            let clk = clock::create_for_testing(test_scenario::ctx(&mut s));
            gradient_registry::submit_gradient(
                &mut reg, 5, 1, walrus_ref(), content_hash(), 1,
                &clk, test_scenario::ctx(&mut s),
            );
            clock::destroy_for_testing(clk);
            test_scenario::return_shared(reg);
        };

        test_scenario::next_tx(&mut s, miner2);
        {
            let mut reg: GradientRegistry = test_scenario::take_shared(&s);
            let clk = clock::create_for_testing(test_scenario::ctx(&mut s));
            gradient_registry::submit_gradient(
                &mut reg, 5, 2, r2_ref(), content_hash(), 0,
                &clk, test_scenario::ctx(&mut s),
            );
            clock::destroy_for_testing(clk);
            test_scenario::return_shared(reg);
        };

        test_scenario::next_tx(&mut s, admin);
        {
            let reg: GradientRegistry = test_scenario::take_shared(&s);
            assert!(gradient_registry::submission_count(&reg, 5) == 2, 0);
            let subs = gradient_registry::list_submissions(&reg, 5);
            let s0 = vector::borrow(&subs, 0);
            let s1 = vector::borrow(&subs, 1);
            assert!(gradient_registry::sub_miner_uid(s0) == 1, 1);
            assert!(gradient_registry::sub_miner_uid(s1) == 2, 2);
            // different backends: first is Walrus (encryption_kind=1), second is R2 (0)
            assert!(gradient_registry::sub_encryption_kind(s0) == 1, 3);
            assert!(gradient_registry::sub_encryption_kind(s1) == 0, 4);
            test_scenario::return_shared(reg);
        };

        test_scenario::end(s);
    }

    #[test]
    fun test_empty_window_returns_empty_list() {
        let owner = @0xA;
        let mut s = test_scenario::begin(owner);

        test_scenario::next_tx(&mut s, owner);
        { gradient_registry::init_for_testing(test_scenario::ctx(&mut s)); };

        test_scenario::next_tx(&mut s, owner);
        {
            let reg: GradientRegistry = test_scenario::take_shared(&s);
            let subs = gradient_registry::list_submissions(&reg, 999);
            assert!(vector::length(&subs) == 0, 0);
            assert!(!gradient_registry::is_committed(&reg, 999), 1);
            test_scenario::return_shared(reg);
        };

        test_scenario::end(s);
    }

    #[test]
    fun test_commit_window_locks_submissions() {
        let admin = @0xA;
        let miner = @0xB;
        let mut s = test_scenario::begin(admin);

        test_scenario::next_tx(&mut s, admin);
        { gradient_registry::init_for_testing(test_scenario::ctx(&mut s)); };

        // Miner submits
        test_scenario::next_tx(&mut s, miner);
        {
            let mut reg: GradientRegistry = test_scenario::take_shared(&s);
            let clk = clock::create_for_testing(test_scenario::ctx(&mut s));
            gradient_registry::submit_gradient(
                &mut reg, 7, 1, walrus_ref(), content_hash(), 1,
                &clk, test_scenario::ctx(&mut s),
            );
            clock::destroy_for_testing(clk);
            test_scenario::return_shared(reg);
        };

        // Admin commits
        test_scenario::next_tx(&mut s, admin);
        {
            let mut reg: GradientRegistry = test_scenario::take_shared(&s);
            assert!(!gradient_registry::is_committed(&reg, 7), 0);
            gradient_registry::commit_window(&mut reg, 7, test_scenario::ctx(&mut s));
            assert!(gradient_registry::is_committed(&reg, 7), 1);
            test_scenario::return_shared(reg);
        };

        test_scenario::end(s);
    }

    #[test]
    #[expected_failure(abort_code = 2)] // E_WINDOW_COMMITTED
    fun test_submit_after_commit_aborts() {
        let admin = @0xA;
        let miner = @0xB;
        let mut s = test_scenario::begin(admin);

        test_scenario::next_tx(&mut s, admin);
        { gradient_registry::init_for_testing(test_scenario::ctx(&mut s)); };

        // Submit once then commit
        test_scenario::next_tx(&mut s, miner);
        {
            let mut reg: GradientRegistry = test_scenario::take_shared(&s);
            let clk = clock::create_for_testing(test_scenario::ctx(&mut s));
            gradient_registry::submit_gradient(
                &mut reg, 3, 1, walrus_ref(), content_hash(), 1,
                &clk, test_scenario::ctx(&mut s),
            );
            clock::destroy_for_testing(clk);
            test_scenario::return_shared(reg);
        };

        test_scenario::next_tx(&mut s, admin);
        {
            let mut reg: GradientRegistry = test_scenario::take_shared(&s);
            gradient_registry::commit_window(&mut reg, 3, test_scenario::ctx(&mut s));
            test_scenario::return_shared(reg);
        };

        // Second submission must abort
        test_scenario::next_tx(&mut s, miner);
        {
            let mut reg: GradientRegistry = test_scenario::take_shared(&s);
            let clk = clock::create_for_testing(test_scenario::ctx(&mut s));
            gradient_registry::submit_gradient(
                &mut reg, 3, 1, walrus_ref(), content_hash(), 1,
                &clk, test_scenario::ctx(&mut s),
            );
            clock::destroy_for_testing(clk);
            test_scenario::return_shared(reg);
        };

        test_scenario::end(s);
    }

    #[test]
    #[expected_failure(abort_code = 3)] // E_UNAUTHORIZED
    fun test_non_admin_commit_aborts() {
        let admin    = @0xA;
        let attacker = @0xB;
        let mut s = test_scenario::begin(admin);

        test_scenario::next_tx(&mut s, admin);
        { gradient_registry::init_for_testing(test_scenario::ctx(&mut s)); };

        // Create the window with a submission so commit_window doesn't hit NOT_FOUND
        test_scenario::next_tx(&mut s, admin);
        {
            let mut reg: GradientRegistry = test_scenario::take_shared(&s);
            let clk = clock::create_for_testing(test_scenario::ctx(&mut s));
            gradient_registry::submit_gradient(
                &mut reg, 1, 0, walrus_ref(), content_hash(), 0,
                &clk, test_scenario::ctx(&mut s),
            );
            clock::destroy_for_testing(clk);
            test_scenario::return_shared(reg);
        };

        // Attacker tries to commit — must abort E_UNAUTHORIZED
        test_scenario::next_tx(&mut s, attacker);
        {
            let mut reg: GradientRegistry = test_scenario::take_shared(&s);
            gradient_registry::commit_window(&mut reg, 1, test_scenario::ctx(&mut s));
            test_scenario::return_shared(reg);
        };

        test_scenario::end(s);
    }

    #[test]
    #[expected_failure(abort_code = 1)] // E_WINDOW_NOT_FOUND
    fun test_commit_nonexistent_window_aborts() {
        let admin = @0xA;
        let mut s = test_scenario::begin(admin);

        test_scenario::next_tx(&mut s, admin);
        { gradient_registry::init_for_testing(test_scenario::ctx(&mut s)); };

        test_scenario::next_tx(&mut s, admin);
        {
            let mut reg: GradientRegistry = test_scenario::take_shared(&s);
            // Window 99 was never opened
            gradient_registry::commit_window(&mut reg, 99, test_scenario::ctx(&mut s));
            test_scenario::return_shared(reg);
        };

        test_scenario::end(s);
    }

    #[test]
    fun test_set_admin_transfers_control() {
        let admin    = @0xA;
        let new_admin = @0xB;
        let mut s = test_scenario::begin(admin);

        test_scenario::next_tx(&mut s, admin);
        { gradient_registry::init_for_testing(test_scenario::ctx(&mut s)); };

        // Transfer admin to new_admin
        test_scenario::next_tx(&mut s, admin);
        {
            let mut reg: GradientRegistry = test_scenario::take_shared(&s);
            gradient_registry::set_admin(&mut reg, new_admin, test_scenario::ctx(&mut s));
            test_scenario::return_shared(reg);
        };

        // Old admin can no longer commit — create a window first
        // new_admin submits
        test_scenario::next_tx(&mut s, new_admin);
        {
            let mut reg: GradientRegistry = test_scenario::take_shared(&s);
            let clk = clock::create_for_testing(test_scenario::ctx(&mut s));
            gradient_registry::submit_gradient(
                &mut reg, 8, 5, walrus_ref(), content_hash(), 1,
                &clk, test_scenario::ctx(&mut s),
            );
            clock::destroy_for_testing(clk);
            test_scenario::return_shared(reg);
        };

        // new_admin commits — should succeed
        test_scenario::next_tx(&mut s, new_admin);
        {
            let mut reg: GradientRegistry = test_scenario::take_shared(&s);
            gradient_registry::commit_window(&mut reg, 8, test_scenario::ctx(&mut s));
            assert!(gradient_registry::is_committed(&reg, 8), 0);
            test_scenario::return_shared(reg);
        };

        test_scenario::end(s);
    }
}
