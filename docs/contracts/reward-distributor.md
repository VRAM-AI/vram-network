# reward\_distributor.move

Emits VRAM tokens each window, distributed proportionally to miners' normalized OpenSkill weights.

## Entry Functions

### `distribute`

```move
public entry fun distribute(
    ledger: &ScoreLedger,
    treasury: &mut TplrTreasury,
    hparams: &Hparams,
    window: u64,
    ctx: &mut TxContext,
)
```

Reads normalized weights from `score_ledger.move` and mints `emission_per_window` tokens, distributing them to each miner's address proportional to their weight.

## Reward Calculation

For each miner $i$ in window $t$:

**Ordinal:**
$$\text{ord}_i = \mu_i - 3\sigma_i$$

**Normalized weight:**
$$w_i = \frac{(\text{ord}_i - \text{ord}_{\min})^2}{\sum_j (\text{ord}_j - \text{ord}_{\min})^2}$$

**Token emission:**
$$\text{reward}_i = w_i \cdot \text{emission\_per\_window}$$

The squaring amplifies the gap between high and median performers.

## Validator Rewards

Validators also receive a fraction of `emission_per_window` proportional to their stake and submission completeness. A validator that submits scores for all $n$ miners receives more than one that submits scores for only $k < n$ miners.

## Error Codes

| Code | Constant | Meaning |
|------|----------|---------|
| 1 | `E_WINDOW_NOT_FINALIZED` | Scores not yet submitted for this window |
| 2 | `E_ALREADY_DISTRIBUTED` | Rewards already distributed for this window |
| 3 | `E_NO_ELIGIBLE_MINERS` | No miners with positive normalized weight |
