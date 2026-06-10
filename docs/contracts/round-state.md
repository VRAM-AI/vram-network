# round\_state.move

Tracks the current window number and anchors checkpoint hashes on-chain. All nodes read `RoundState` to synchronize on the current window.

## Shared Object

`RoundState` — created at package publish; shared with all nodes.

## Key Types

```move
public struct RoundState has key {
    id: UID,
    current_window: u64,
    genesis_ms: u64,             // Timestamp of window 0
    checkpoint_hash: vector<u8>, // SHA-256 hash of latest anchored checkpoint
    checkpoint_window: u64,      // Window at which the checkpoint was anchored
    admin: address,
}
```

## Window Derivation

The current window is derived from the current Sui clock timestamp:

$$\text{window} = \left\lfloor \frac{\text{now\_ms} - \text{genesis\_ms}}{\text{window\_duration\_ms}} \right\rfloor$$

This is deterministic — all nodes compute the same window from the same timestamp and genesis, without any coordinator.

## Entry Functions

### `advance_window`

```move
public entry fun advance_window(
    state: &mut RoundState,
    hparams: &slcl::hparams::Hparams,
    clock: &sui::clock::Clock,
    ctx: &mut TxContext,
)
```

Advances `current_window` to the current derived window. Called by any node at window boundaries.

### `anchor_checkpoint`

```move
public entry fun anchor_checkpoint(
    state: &mut RoundState,
    checkpoint_hash: vector<u8>,
    window: u64,
    clock: &sui::clock::Clock,
    ctx: &mut TxContext,
)
```

Records a new checkpoint hash on-chain. Called by the aggregator every `checkpoint_frequency` windows. The `checkpoint_hash` is SHA-256 of the serialized checkpoint bytes.

Miners read this hash to verify they loaded the correct checkpoint before training.

`score_ledger.move` reads this hash to verify that submitted scores reference the current checkpoint.

## View Functions

```move
public fun current_window(state: &RoundState): u64
public fun get_checkpoint_hash(state: &RoundState): &vector<u8>
public fun get_checkpoint_window(state: &RoundState): u64
```

## Error Codes

| Code | Constant | Meaning |
|------|----------|---------|
| 1 | `E_WINDOW_NOT_ADVANCED` | Attempted to anchor checkpoint before advancing window |
| 2 | `E_CHECKPOINT_TOO_OLD` | Checkpoint window is behind current window |

## Checkpoint Lifecycle

```
Every checkpoint_frequency windows:

Aggregator:
  1. Downloads top-G miner gradients
  2. Aggregates into merged model state
  3. Uploads checkpoint bytes to R2
  4. Computes SHA-256 hash of checkpoint bytes
  5. Calls anchor_checkpoint on-chain

Miners (next window):
  1. Read checkpoint_hash from RoundState
  2. Download checkpoint bytes from R2
  3. Verify SHA-256(bytes) == checkpoint_hash
  4. Load model weights from bytes
```

The on-chain hash prevents a malicious aggregator from serving a different model than what was committed.
