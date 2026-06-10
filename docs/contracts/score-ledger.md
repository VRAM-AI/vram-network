# score\_ledger.move

Verifies enclave Ed25519 signatures on score submissions and stores per-window scores. This is the cheapest on-chain verification step in the pipeline — one signature check per submission.

## Design

> v0.4 change: instead of verifying a full Nitro attestation document on every submission (expensive), we verify a simple Ed25519 signature against the enclave public key stored in `enclave_registry.move` (cheap).
>
> The attestation was already verified during enclave registration. Here we just confirm that the scores came from that registered enclave.

## Shared Object

`ScoreLedger` — created at package publish; shared with all nodes.

## Key Types

```move
public struct ScoreLedger has key {
    id: UID,
    submissions: Table<u64, vector<ValidatorSubmission>>,
    scores: Table<u64, PeerScore>,
    admin: address,
}

public struct ValidatorSubmission has store, drop {
    validator_uid: u64,
    window: u64,
    scores: VecMap<u64, u64>,          // miner_uid → score (1e9 fixed-point)
    stake_at_submission: u64,
    submitted_at_ms: u64,
    enclave_signature: vector<u8>,     // Ed25519 signature from registered enclave
    checkpoint_hash: vector<u8>,
}

public struct PeerScore has store, copy, drop {
    uid: u64,
    openskill_mu: u64,                 // 1e9 fixed-point
    openskill_sigma: u64,              // 1e9 fixed-point
    mu_generalization: u64,
    peer_score: u64,
    normalized_weight: u64,            // 1e9 fixed-point
    last_updated_window: u64,
}
```

## Entry Functions

### `submit_scores`

```move
public entry fun submit_scores(
    ledger: &mut ScoreLedger,
    enclave_registry: &slcl::enclave_registry::EnclaveRegistry,
    validator_uid: u64,
    window: u64,
    scores_keys: vector<u64>,
    scores_values: vector<u64>,
    stake_at_submission: u64,
    submitted_at_ms: u64,
    enclave_signature: vector<u8>,
    signed_payload_bytes: vector<u8>,
    checkpoint_hash: vector<u8>,
    expected_checkpoint_hash: vector<u8>,
    ctx: &mut TxContext,
)
```

Verifies and records a batch of miner scores for a window:

1. **Enclave check** — `enclave_registry::is_registered(validator_uid)` must be true
2. **Signature verify** — `ed25519_verify(signature, enclave_pubkey, payload)` must pass
3. **Checkpoint check** — `checkpoint_hash == expected_checkpoint_hash`
4. **Record** — appends submission to `submissions[window]`

The `signed_payload_bytes` is a canonical CBOR encoding of `(window, checkpoint_hash, scores)` — the same encoding the Nautilus enclave uses when signing. The contract does not re-encode; it just verifies the signature on whatever bytes the validator provides, making the validator responsible for providing the correct payload.

## Error Codes

| Code | Constant | Meaning |
|------|----------|---------|
| 1 | `E_SCORE_ALREADY_SUBMITTED` | Validator already submitted for this window |
| 2 | `E_WINDOW_FINALIZED` | Window is past the submission deadline |
| 3 | `E_SIGNATURE_INVALID` | Ed25519 verification failed |
| 4 | `E_CHECKPOINT_MISMATCH` | Checkpoint hash does not match chain |
| 5 | `E_ENCLAVE_NOT_REGISTERED` | No enclave registered for this validator |

## Fixed-Point Representation

All score values use **1e9 fixed-point** (i.e., `1_000_000_000` represents `1.0`). This is standard throughout `slcl` — always add the comment `// 1e9 = 1.0` when working with these values.

## Score Aggregation

After all validators submit for a window, `reward_distributor.move` reads the `scores` table to compute normalized weights. Multiple validators may submit scores for the same miner — the aggregation function stake-weights each validator's submission:

$$s_i^{\text{final}} = \frac{\sum_v \text{stake}_v \cdot s_i^v}{\sum_v \text{stake}_v}$$
