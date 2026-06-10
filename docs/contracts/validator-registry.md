# validator\_registry.move

Manages validator registration, stake tracking, and provides the access control interface used by `seal_policy.move`.

## Shared Object

`ValidatorRegistry` — created at package publish; shared with all nodes and Seal key servers.

## Key Types

```move
public struct ValidatorRegistry has key {
    id: UID,
    validators: Table<u64, ValidatorInfo>,
    admin: address,
}

public struct ValidatorInfo has store {
    uid: u64,
    owner: address,
    stake: u64,
    registered_at_ms: u64,
    active: bool,
    enclave_registered: bool,
}
```

## Entry Functions

### `register_validator`

```move
public entry fun register_validator(
    registry: &mut ValidatorRegistry,
    uid: u64,
    stake: u64,
    clock: &sui::clock::Clock,
    ctx: &mut TxContext,
)
```

Registers a validator with the given UID and stake amount.

### `update_stake`

```move
public entry fun update_stake(
    registry: &mut ValidatorRegistry,
    uid: u64,
    new_stake: u64,
    ctx: &mut TxContext,
)
```

Updates a validator's stake. Used when a validator adds or removes stake.

### `set_active`

```move
public entry fun set_active(
    registry: &mut ValidatorRegistry,
    uid: u64,
    active: bool,
    ctx: &mut TxContext,
)
```

Activates or deactivates a validator. Only callable by the validator's registered owner.

## View Functions Used by `seal_policy.move`

```move
public fun is_registered(registry: &ValidatorRegistry, uid: u64): bool
public fun get_stake(registry: &ValidatorRegistry, uid: u64): u64
public fun is_active(registry: &ValidatorRegistry, uid: u64): bool
```

These three functions form the access control interface. `seal_policy.move` calls all three in `seal_approve`.

## Error Codes

| Code | Constant | Meaning |
|------|----------|---------|
| 1 | `E_VALIDATOR_NOT_FOUND` | UID not registered |
| 2 | `E_NOT_OWNER` | Caller is not the validator's owner |
| 3 | `E_INSUFFICIENT_STAKE` | Stake below `min_validator_stake` |
| 4 | `E_ALREADY_REGISTERED` | UID already registered |

## Relationship to Seal

The Seal key servers simulate a PTB that calls `seal_approve` in `seal_policy.move`. That function calls back into `validator_registry.move` to check:

1. `is_registered(uid)` — the validator exists
2. `get_stake(uid) >= min_validator_stake` — the validator has sufficient stake
3. `is_active(uid)` — the validator has not been deactivated

All three must pass. If any check fails, the key servers refuse to release IBE key fragments.
