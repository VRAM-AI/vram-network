# Hyperparameters

All hyperparameters are stored on-chain in `hparams.move` and are governance-updatable. No node restart is required — nodes read hyperparameters from the chain at the start of each window.

## Parameter Reference

| Parameter | Default | Description |
|-----------|---------|-------------|
| `window_duration_ms` | 600,000 | Window length in milliseconds (10 minutes) |
| `put_window_open_ms` | 480,000 | Gradient upload deadline within a window (8 minutes) |
| `topk_compression` | 32 | Number of top-k DCT coefficients transmitted per gradient |
| `top_g` | 15 | Number of top-ranked peers selected for aggregation |
| `openskill_beta` | 25/6 ≈ 4.17 | OpenSkill performance variance parameter |
| `openskill_tau` | 25/300 ≈ 0.083 | OpenSkill drift per window (skill decay) |
| `emission_per_window` | 1,000,000,000,000 | Tokens emitted per window (in base units) |
| `checkpoint_frequency` | 100 | Windows between checkpoint anchoring |
| `min_miner_stake` | 1,000,000,000 | Minimum stake to register as miner (1 SUI = 1e9 MIST) |
| `min_validator_stake` | 10,000,000,000 | Minimum stake to register as validator (10 SUI) |
| `validator_offset` | 2 | Number of windows a new validator must wait before evaluating |
| `gauntlet_gamma` | 0.99 | Decay factor applied to gauntlet scores each window |
| `sync_threshold` | 3 | Minimum gradient size ratio for a peer to pass the sync fast-eval check |

## Understanding Key Parameters

### `window_duration_ms` and `put_window_open_ms`

The window is split into two phases:

```
t=0ms                    t=480,000ms         t=600,000ms
  │                           │                    │
  ├── training phase ─────────┤── score phase ─────┤
  │   (miners train + upload) │   (validators eval) │
```

Miners must upload their gradient before `put_window_open_ms`. After that deadline, validators begin downloading and evaluating. This separation prevents validators from racing to evaluate while miners are still uploading.

### `topk_compression`

Controls the compression ratio. With `topk_compression = 32`, only 32 DCT coefficients are transmitted per gradient tensor. Higher values mean better gradient quality but larger uploads.

The effective compression ratio depends on model size. For a 1B-parameter model, `topk_compression = 32` provides extremely aggressive compression (~99.997% reduction); validators tolerate some approximation error because the loss delta evaluation captures the net effect.

### `top_g`

Each window, only the top `top_g` miners (by OpenSkill ordinal) contribute their gradients to the aggregated checkpoint. This prevents low-quality gradients from polluting the shared model state.

### `openskill_beta` and `openskill_tau`

- **`beta`** controls how much a single window's result can move a miner's skill estimate. Higher beta → more uncertainty → slower convergence to stable ratings.
- **`tau`** controls how quickly ratings drift toward the prior when a miner is inactive. Higher tau → ratings decay faster → more emphasis on recent windows.

### `checkpoint_frequency`

Checkpoints are anchored on-chain every `checkpoint_frequency` windows. In between, miners load the most recent available checkpoint. Increasing this reduces on-chain storage costs but increases the amount of training state that may be lost if an aggregator fails.

## Governance

Hyperparameters are updated via the `update_hparams` entry function in `hparams.move`, which requires the `HparamsAdminCap`. In production, governance will route this through a multisig or DAO structure.

Changes take effect at the next window boundary — nodes read fresh hyperparameters at the start of each window loop.
