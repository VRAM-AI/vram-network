// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

/// # SealPolicy (v0.4)
///
/// Defines the Seal access control policy for SLCL credential encryption.
///
/// Seal key servers call `seal_approve` to check if a requester is authorized
/// to decrypt. This function checks that the requester is a registered validator
/// with sufficient stake in the ValidatorRegistry.
///
/// This is the CORRECT Seal integration pattern:
/// - The package ID of this module is the Seal identity namespace.
/// - seal_approve is an entry function that key servers simulate via PTB.
/// - If the function executes without aborting, access is granted.

module slcl::seal_policy {
    use sui::tx_context::{Self, TxContext};
    use slcl::validator_registry::{Self, ValidatorRegistry};

    /// Error codes
    const E_NOT_A_VALIDATOR: u64 = 1;
    const E_INSUFFICIENT_STAKE: u64 = 2;

    public fun err_not_a_validator(): u64 { E_NOT_A_VALIDATOR }
    public fun err_insufficient_stake(): u64 { E_INSUFFICIENT_STAKE }

    /// Entry function called by Seal key servers to check decryption access.
    ///
    /// Key servers build a PTB (Programmable Transaction Block) that calls
    /// this function with the requester's address. If the function completes
    /// without aborting, the key server releases its key share.
    ///
    /// # Access rule
    ///
    /// The requester must be a registered active validator
    /// with stake >= min_validator_stake.
    ///
    /// # Arguments
    /// * `id` - the Seal identity bytes being decrypted (for logging/audit)
    /// * `validator_registry` - shared ValidatorRegistry object
    public entry fun seal_approve<T>(
        id: vector<u8>,
        validator_registry: &ValidatorRegistry<T>,
        ctx: &mut TxContext,
    ) {
        let sender = tx_context::sender(ctx);

        // Check that sender is a registered, active validator.
        assert!(
            validator_registry::is_registered_validator(validator_registry, sender),
            E_NOT_A_VALIDATOR
        );

        // Check that sender has sufficient stake.
        let stake = validator_registry::get_stake(validator_registry, sender);
        assert!(stake >= validator_registry::min_stake(validator_registry), E_INSUFFICIENT_STAKE);

        // Identity bytes are used by the calling party to scope the IBE identity;
        // no further action needed here.
        let _ = id;
    }
}
