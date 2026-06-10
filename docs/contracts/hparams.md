# hparams.move

Stores all system hyperparameters on-chain. Nodes read these at the start of each window — no restart required for parameter changes.

## Shared Object

`Hparams` — created at package publish with default values; updated via governance.

## Stored Parameters

```move
public struct Hparams has key {
    id: UID,
    window_duration_ms: u64,      // default: 600_000
    put_window_open_ms: u64,      // default: 480_000
    topk_compression: u64,        // default: 32
    top_g: u64,                   // default: 15
    openskill_beta: u64,          // 1e9 fixed-point, default: 4_166_666_667 (25/6)
    openskill_tau: u64,           // 1e9 fixed-point, default: 83_333_333 (25/300)
    emission_per_window: u64,     // default: 1_000_000_000_000
    checkpoint_frequency: u64,    // default: 100
    min_miner_stake: u64,         // MIST, default: 1_000_000_000 (1 SUI)
    min_validator_stake: u64,     // MIST, default: 10_000_000_000 (10 SUI)
    expected_pcr0: vector<u8>,    // 48 bytes
    expected_pcr1: vector<u8>,    // 48 bytes
    expected_pcr2: vector<u8>,    // 48 bytes
    admin: address,
}
```

## Entry Functions

### `update_hparams`

```move
public entry fun update_hparams(
    _cap: &HparamsAdminCap,
    hparams: &mut Hparams,
    window_duration_ms: u64,
    put_window_open_ms: u64,
    // ... all parameters
    ctx: &mut TxContext,
)
```

Requires `HparamsAdminCap`. In production, this capability is held by a governance multisig.

### `update_pcrs`

```move
public entry fun update_pcrs(
    _cap: &HparamsAdminCap,
    hparams: &mut Hparams,
    pcr0: vector<u8>,
    pcr1: vector<u8>,
    pcr2: vector<u8>,
    ctx: &mut TxContext,
)
```

Separate function for updating expected PCR values when the enclave binary changes. Requires admin cap.

## View Functions

```move
public fun window_duration_ms(h: &Hparams): u64
public fun put_window_open_ms(h: &Hparams): u64
public fun topk_compression(h: &Hparams): u64
public fun top_g(h: &Hparams): u64
public fun openskill_beta(h: &Hparams): u64
public fun openskill_tau(h: &Hparams): u64
public fun emission_per_window(h: &Hparams): u64
public fun checkpoint_frequency(h: &Hparams): u64
public fun min_miner_stake(h: &Hparams): u64
public fun min_validator_stake(h: &Hparams): u64
public fun expected_pcrs(h: &Hparams): (&vector<u8>, &vector<u8>, &vector<u8>)
```

For a full description of each parameter, see [Hyperparameters](../hparams.md).
