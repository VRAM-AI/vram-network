# Incentive Mechanism

VRAM HUB uses a two-stage incentive pipeline: (1) loss-delta scoring evaluated inside a TEE, and (2) OpenSkill Bayesian rating that converts per-window scores into stable reward weights.

## Miner Contribution: Loss Delta

For each miner $i$ evaluated in window $t$, a Nautilus enclave computes:

$$s_i^t = L(\theta^t;\, D_i) - L(\theta^t + \delta_i;\, D_i)$$

where:
- $\theta^t$ — model parameters at the start of window $t$ (the anchored checkpoint)
- $D_i$ — miner $i$'s deterministically assigned data batch
- $\delta_i$ — miner $i$'s compressed gradient
- $L$ — cross-entropy loss

A positive $s_i^t$ means miner $i$'s gradient improved the model on their assigned batch. A negative value means it hurt.

### Data Assignment

Each miner receives a unique but reproducible data subset:

$$\text{seed} = \text{SHA256}(\text{uid} \,\|\, \text{window})$$
$$\text{pages} = \text{Sample}(\text{dataset},\, \text{seed},\, \text{batch\_size})$$

Validators use the same seed to verify which data was assigned — making the evaluation deterministic and independent of the miner's self-reported assignment.

### Gradient Compression

Miners apply momentum accumulation before compression:

**Momentum buffer update:**

$$m_i^{t+1} = \gamma \cdot m_i^t + \eta \cdot g_i^t$$

**Weight decay:**

$$\theta^{t+1} = (1 - \lambda)\,\theta^t$$

**Top-k DCT compression:**

$$\tilde{g}_i^t = \text{TopK}\!\left(\text{DCT}(m_i^{t+1}),\; k\right)$$

where $k$ is set by the `topk_compression` hyperparameter. Only the top $k$ DCT coefficients (by magnitude) are transmitted, dramatically reducing upload bandwidth.

## OpenSkill Rating

Raw per-window loss deltas are noisy. VRAM HUB uses the **Plackett-Luce OpenSkill model** to convert noisy window scores into stable Bayesian skill estimates $(\mu_i, \sigma_i)$.

### Rating Update (Per Window)

Miners are ranked by their loss delta $s_i^t$. The OpenSkill update is:

**Team performance scale:**

$$c = \sqrt{\sum_i \sigma_i^2 + \beta^2 + n\beta^2}$$

**Omega factor (rank-weighted gradient):**

$$\omega_i = \frac{1}{c}\!\left(1 - \sum_{q=0}^{r_i} \frac{e^{\mu_i/c}}{A_q}\right), \quad A_q = \sum_{j:\,\text{rank}(j)\,\geq\, q} e^{\mu_j/c}$$

**Mu update:**

$$\mu_i^{t+1} = \mu_i^t + \sigma_i^2 \cdot \omega_i$$

**Sigma update:**

$$\sigma_i^{t+1} = \sigma_i^t \cdot \sqrt{1 - \sigma_i^2 \cdot \delta_i}$$

**Ordinal (conservative lower bound):**

$$\text{ord}_i = \mu_i - 3\sigma_i$$

The ordinal represents a conservative estimate: even accounting for uncertainty, the miner is expected to be at least this good.

### Why Bayesian Ratings?

Simple averaging of per-window scores is vulnerable to:
- **Noise amplification** — a single anomalous window dominates
- **New miner disadvantage** — new miners need many windows to establish reputation

OpenSkill addresses both:
- New miners start with high $\sigma$ (high uncertainty), which shrinks as more data accumulates
- A consistently good miner has high $\mu$ and low $\sigma$, producing a high ordinal
- A miner that was good but has degraded will see $\mu$ decay via the drift term $\tau$

## Reward Distribution

Rewards are distributed proportional to squared ordinals (normalized):

$$w_i = \frac{(\text{ord}_i - \text{ord}_{\min})^2}{\sum_j (\text{ord}_j - \text{ord}_{\min})^2}$$

The squaring amplifies the gap between top and median performers — incentivizing sustained high performance rather than mediocrity.

Each window, `reward_distributor.move` emits `emission_per_window` VRAM tokens distributed by $w_i$.

## Validator Incentives

Validators earn a share of the `emission_per_window` allocation in proportion to their stake and submission completeness. Validators that submit scores for fewer miners receive proportionally less.

This ensures validators are incentivized to evaluate all registered miners, not just the cheapest ones.

## VRAM Token Emission Schedule

**Token:** VRAM · **Hard cap:** 21,000,000 VRAM · **Decimals:** 9

**Window cadence:** 10 minutes → **52,596 windows/year**

Halving is **supply-based**, not time-based. It triggers when cumulative mining tokens issued cross a threshold — independent of clock time:

| Phase | Supply Threshold | Est. Duration | Emission/Window | Phase Total | Cumulative |
|-------|-----------------|---------------|-----------------|-------------|------------|
| 1 (Genesis) | 0 → 7M issued | ~1.9 years | 70 VRAM | 7.0M | 7.0M |
| 2 | 7M → 10.5M (cap) | ~1.9 more years | 35 VRAM | 3.5M | 10.5M |

Mining allocation exhausts at **10.5M VRAM** (50% of hard cap). `can_emit()` returns false at that point and emission stops permanently.

The remaining 50% of supply is pre-minted at TGE: Treasury 30% (6m cliff, 48m vest) · Team 8% (12m cliff, 36m vest) · Liquidity 7% (instant) · Airdrop 5%.

**Distribution split per window:**

| Recipient | BPS | Amount (Phase 1) | Mechanism |
|-----------|-----|-----------------|-----------|
| Miners | 7143 | ~50 VRAM | Proportional to OpenSkill normalized weight $w_i$ |
| Validators | 2857 | ~20 VRAM | Proportional to stake × submission completeness |

> **Note:** v0.4 (current) sends 100% to miners. The 72/18/10 split ships in v0.5 alongside the validator reward module.

**Testnet pool (live as of 2026-03-30):**

| Parameter | Value |
|-----------|-------|
| RewardPool ID | `0x576ebeb78449ad46ef70dc3c5ca4e38d178846610bd7cf9f0764ae2f1dc0fe93` |
| Initial deposit | 6,000,000 VRAM |
| Emission/window | 1,200 VRAM |
| Runway | ~5,000 windows (~34 days) |

## Anti-Gaming Properties

| Attack | Why It Fails |
|--------|-------------|
| Upload a gradient that minimizes loss on your data but hurts others | Other miners' data is different; your score is only on your assigned batch |
| Copy another miner's gradient | Loss delta is evaluated on your assigned data — copying another miner's gradient for their data produces zero or negative delta on yours |
| Submit a pre-computed gradient from a prior window | Checkpoint hash is checked; gradients computed from a stale checkpoint will produce wrong loss |
| Collude with a validator | Validator cannot forge enclave signatures; signed scores must come from the registered enclave |
