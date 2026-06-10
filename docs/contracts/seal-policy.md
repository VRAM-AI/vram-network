# seal\_policy.move

Provides the single `seal_approve` entry point used by Sui Seal key servers to gate IBE key release. This is the access control boundary for all credential decryption in VRAM HUB.

## Design

`seal_policy.move` is intentionally minimal. It has no state and no AdminCap. It is purely an access control gate that delegates all state lookups to `validator_registry.move`.

## Entry Function

### `seal_approve`

```move
public entry fun seal_approve(
    id: vector<u8>,
    registry: &slcl::validator_registry::ValidatorRegistry,
    validator_uid: u64,
    hparams: &slcl::hparams::Hparams,
    ctx: &TxContext,
)
```

Called by Seal key servers when a validator requests IBE key fragments. The key servers simulate this PTB without submitting it to chain — the simulation result (pass/abort) determines whether fragments are released.

**Checks performed:**

1. `validator_registry::is_registered(registry, validator_uid)` — validator exists
2. `validator_registry::get_stake(registry, validator_uid) >= hparams.min_validator_stake` — sufficient stake
3. `validator_registry::is_active(registry, validator_uid)` — not deactivated

If all checks pass, the function returns normally (Seal key servers release fragments).
If any check fails, the function aborts (Seal key servers refuse).

## Error Codes

| Code | Constant | Meaning |
|------|----------|---------|
| 1 | `E_NOT_VALIDATOR` | UID not registered as validator |
| 2 | `E_INSUFFICIENT_STAKE` | Stake below `min_validator_stake` |
| 3 | `E_INACTIVE` | Validator is marked inactive |

## How Seal IBE Works

```
Miner encrypts R2 credentials:
  ciphertext = IBE_encrypt(plaintext, identity="vram-validators")
  stored in peer_registry.move

Validator decrypts:
  1. Constructs PTB: [seal_approve(id, registry, validator_uid, hparams)]
  2. Signs PTB with session key (short-lived Ed25519 key)
  3. Sends PTB + session key to each Seal key server
  4. Each key server simulates the PTB:
     - If seal_approve passes → returns IBE key fragment
     - If seal_approve aborts → returns error
  5. Validator collects t-of-n fragments → reconstructs IBE master key fragment
  6. Decrypts ciphertext using IBE key

Result: only staked, active validators can read miner R2 credentials
```

## Security Properties

- **No trusted third party** — the threshold scheme distributes trust; no single key server can leak credentials
- **Stake-gated** — a validator that loses stake or is deactivated immediately loses decryption access
- **Simulation, not execution** — the PTB is never submitted on-chain; key servers only simulate it to check access control
- **Revocable** — deactivating a validator in `validator_registry.move` immediately prevents future decryption
