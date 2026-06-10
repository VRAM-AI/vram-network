# Smart Contracts Overview

VRAM HUB's on-chain logic is implemented as a Sui Move package (`slcl`) consisting of 9 modules. All modules are deployed as a single atomic package upgrade.

## Deployed Addresses (Devnet)

| Object | ID |
|--------|----|
| Package | `0x794814c27e91bb927613253331b17b82906d4beaa1e5329f027dc8765c06359a` |
| PeerRegistry | `0xa4eec521...` |
| ValidatorRegistry | `0x5bf4cc62...` |
| EnclaveRegistry | `0x8e894a34...` |
| ScoreLedger | `0x44fa4a3a...` |
| RoundState | `0xd112fa48...` |
| Hparams | `0xbc39eb25...` |

Full object IDs are in `.env.example`.

## Module Responsibilities

| Module | Shared Object | AdminCap | Purpose |
|--------|--------------|----------|---------|
| [`peer_registry`](peer-registry.md) | `PeerRegistry` | `PeerRegistryAdminCap` | Peer registration + IBE-encrypted R2 credentials |
| [`validator_registry`](validator-registry.md) | `ValidatorRegistry` | `ValidatorRegistryAdminCap` | Validator stake + Seal access control |
| [`enclave_registry`](enclave-registry.md) | `EnclaveRegistry` | `EnclaveRegistryAdminCap` | Nitro enclave PCR + public key storage |
| [`score_ledger`](score-ledger.md) | `ScoreLedger` | `LedgerAdminCap` | Ed25519 verification + score storage |
| [`round_state`](round-state.md) | `RoundState` | `RoundStateAdminCap` | Window state + checkpoint hash anchoring |
| [`hparams`](hparams.md) | `Hparams` | `HparamsAdminCap` | On-chain hyperparameters |
| [`reward_distributor`](reward-distributor.md) | — | `RewardAdminCap` | Per-window token emission |
| [`seal_policy`](seal-policy.md) | — | — | IBE access gate (`seal_approve`) |
| `vram_token` | — | `TplrTokenAdminCap` | VRAM reward token (fungible asset) |

## Design Conventions

Every module in `slcl` follows these conventions:

- **Module-level doc comment** — describes what the module does and references the design spec
- **Named error constants** — all `abort` statements use `E_` prefixed constants (e.g., `E_SIGNATURE_INVALID`)
- **AdminCap pattern** — mutable admin operations require the caller to present an `AdminCap` capability object
- **Shared objects** — coordination objects (`PeerRegistry`, `ScoreLedger`, etc.) are shared, readable by all
- **No abort without constant** — all assertions use named error codes, never raw integers

## Access Control

```
seal_policy.move
  └── seal_approve (called by Seal key servers to gate IBE key release)
        ├── validator_registry.move → is_registered(validator_uid)
        ├── validator_registry.move → has_sufficient_stake(validator_uid, min_validator_stake)
        └── validator_registry.move → is_active(validator_uid)

score_ledger.move → submit_scores
  ├── enclave_registry.move → is_registered(validator_uid)
  ├── enclave_registry.move → get_enclave_pubkey(validator_uid)
  ├── ed25519::ed25519_verify(signature, pubkey, payload)
  └── round_state.move → checkpoint_hash matches

reward_distributor.move → distribute
  └── score_ledger.move → get_normalized_weights(window)
```

## Upgrade Policy

The package is deployed with the `slcl` address. Upgrades require the package owner and must not break backward compatibility with existing shared objects (Sui's upgrade rules apply).

Hyperparameter changes do not require a package upgrade — they are handled through `hparams.move`'s governance functions.

## Building and Deploying

```bash
# Build contracts (from repo root)
cd contracts
sui move build --build-env devnet

# Deploy to devnet
./scripts/deploy-testnet.sh devnet

# Deploy to testnet
./scripts/deploy-testnet.sh testnet
```

The deploy script publishes the package and creates all shared objects, printing their IDs to stdout. Copy these into `.env`.
