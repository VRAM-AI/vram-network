// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

#[test_only]
module slcl::seal_policy_tests {
    use sui::test_scenario;
    use slcl::seal_policy;
    use slcl::validator_registry::{Self, ValidatorRegistry};

    public struct TOK has drop {}

    #[test]
    fun test_seal_approve_registered_validator() {
        let validator = @0xB;
        let mut s = test_scenario::begin(validator);

        test_scenario::next_tx(&mut s, validator);
        {
            validator_registry::create_registry_for_testing<TOK>(
                10_000_000_000, test_scenario::ctx(&mut s),
            );
        };

        // Register the validator
        test_scenario::next_tx(&mut s, validator);
        {
            let mut registry: ValidatorRegistry<TOK> = test_scenario::take_shared(&s);
            validator_registry::register_validator_for_testing(
                &mut registry, 0, 10_000_000_000,
                test_scenario::ctx(&mut s),
            );
            test_scenario::return_shared(registry);
        };

        // seal_approve — should succeed
        test_scenario::next_tx(&mut s, validator);
        {
            let registry: ValidatorRegistry<TOK> = test_scenario::take_shared(&s);
            seal_policy::seal_approve<TOK>(
                b"test_identity",
                &registry,
                test_scenario::ctx(&mut s),
            );
            test_scenario::return_shared(registry);
        };

        test_scenario::end(s);
    }

    #[test]
    #[expected_failure(abort_code = 1, location = slcl::seal_policy)]
    fun test_seal_approve_unregistered_fails() {
        let admin = @0xA;
        let unregistered = @0xC;
        let mut s = test_scenario::begin(admin);

        test_scenario::next_tx(&mut s, admin);
        {
            validator_registry::create_registry_for_testing<TOK>(
                10_000_000_000, test_scenario::ctx(&mut s),
            );
        };

        // Unregistered address tries to seal_approve — must abort
        test_scenario::next_tx(&mut s, unregistered);
        {
            let registry: ValidatorRegistry<TOK> = test_scenario::take_shared(&s);
            seal_policy::seal_approve<TOK>(
                b"test_identity",
                &registry,
                test_scenario::ctx(&mut s),
            );
            test_scenario::return_shared(registry);
        };

        test_scenario::end(s);
    }
}
