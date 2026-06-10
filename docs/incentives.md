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

**Token:** VRAM · **Hard cap:** 500,000,000 VRAM · **Decimals:** 9

**Window cadence:** 10 minutes → **52,560 windows/year**

The emission schedule follows a TAO/Bitcoin-style halving with 4-year epochs:

| Epoch | Window Range | Duration | Emission/Window | VRAM Emitted | Cumulative |
|-------|-------------|----------|-----------------|--------------|------------|
| 1 (Genesis) | 1 – 210,240 | Yr 1–4 | 1,200 VRAM | ~252M | ~252M |
| 2 | 210,241 – 420,480 | Yr 5–8 | 600 VRAM | ~126M | ~378M |
| 3 | 420,481 – 630,720 | Yr 9–12 | 300 VRAM | ~63M | ~441M |
| 4 | 630,721 – 840,960 | Yr 13–16 | 150 VRAM | ~31.5M | ~472M |
| 5+ | … | … | halving… | ~28M… | → 500M |

The geometric series converges to **~504M ≈ 500M** (governance will halt minting at 500M).

Halving is governance-controlled via `hparams::update_emission` — the deployer calls this at each epoch boundary.

**Distribution split per window (v0.5+):**

| Recipient | Share | Amount (Genesis) | Mechanism |
|-----------|-------|-----------------|-----------|
| Miners | 72% | 864 VRAM | Proportional to OpenSkill normalized weight $w_i$ |
| Validators | 18% | 216 VRAM | Proportional to stake × submission completeness |
| Protocol treasury | 10% | 120 VRAM | Fixed address, funds future development |

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
