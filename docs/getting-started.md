# Getting Started with VRAM HUB

New to VRAM HUB? This guide helps you choose the right path.

---

## What is VRAM HUB?

VRAM HUB pays GPU owners in **VRAM tokens** for contributing compute to AI model training. A hardware-verified score (from an AWS Nitro Enclave) decides how much each contributor earns — you can't fake a good score.

```
You run a GPU  →  Train a piece of the model  →  Score is verified by TEE  →  Earn VRAM tokens
```

---

## Who Are You?

### I want to earn VRAM with my GPU

→ **[Run a Miner](miners/setup.md)**

- Minimum: any machine with a GPU (or even CPU for testing)
- Storage: free [Cloudflare R2](https://dash.cloudflare.com) account
- Wallet: Sui testnet wallet (free)
- Time to first training window: ~30 minutes

### I want to be a validator and earn more

→ **[Run a Validator](validators/setup.md)**

- Requires: AWS EC2 instance with Nitro Enclave support (`c5.xlarge` minimum)
- Cost: ~$36/month spot pricing
- Validators score miners and earn 18% of each window's emission

### I'm a developer who wants to understand the protocol

→ **[Architecture Overview](architecture.md)** then **[ONBOARDING.md](../ONBOARDING.md)**

### I want to bring my own PyTorch model

→ **[Python Sidecar](miners/running.md#python-sidecar--bring-your-own-model)**

Any HuggingFace causal LM works. The sidecar handles training; VRAM handles rewards.

### I want to explore the network

→ **[VRAMScan](vramscan/overview.md)** at `http://localhost:4322` (after `cd vramscan && npm run dev`)

---

## Quick Test (No GPU, No Wallet — 5 Minutes)

Run a full local simulation to see how the protocol works before committing any hardware:

```bash
git clone https://github.com/VRAM-AI/VRAM-HUB.git
cd VRAM-HUB

# Terminal 1: full simulation (6 miners, 3 validators, toy LLM)
cargo run -p vramhub-local-demo

# Terminal 2: block explorer
cd vramscan && npm install && npm run dev
```

Open **http://localhost:4322** and watch:
- Miners competing over training windows
- OpenSkill ratings diverging as better miners pull ahead
- VRAM token emission per window

---

## Testnet Status

Everything below is live on Sui testnet. No setup required on the contract side.

| Component | Status |
|-----------|--------|
| Smart contracts | ✅ Deployed `0xaff18bf6…` |
| VRAM token + RewardPool | ✅ 6M VRAM funded |
| Miner registration | ✅ Auto-register on first run |
| GPU training (Candle, CUDA, Metal) | ✅ Ready |
| Python sidecar (any HuggingFace model) | ✅ Ready |
| Validator scoring (requires AWS Nitro) | ⏳ First validator coming soon |
| Token payouts | ⏳ Unlocks with first validator |

---

## Tokenomics at a Glance

| Parameter | Value |
|-----------|-------|
| Hard cap | 500,000,000 VRAM |
| Window duration | 10 minutes |
| Emission per window | 1,200 VRAM |
| Miner share (v0.4) | 100% |
| Miner share (v0.5+) | 72% miners · 18% validators · 10% treasury |
| Halving | Every 4 years (governance vote) |

Miner rewards are proportional to each miner's **OpenSkill normalized weight**. The weight is derived from your gradient quality score across multiple windows — not just the most recent one.

---

## Frequently Asked Questions

**Do I need a powerful GPU?**
No. The default nano-GPT adapter (10M parameters) trains on CPU. You'll earn less than miners with GPUs, but you can start testing immediately without specialized hardware.

**Is testnet VRAM worth anything?**
Not yet. Testnet is for testing the protocol. Mainnet launch will be announced via the project's official channels.

**What happens if my miner goes offline?**
Your OpenSkill sigma (uncertainty) increases, which lowers your normalized weight. When you come back online, your weight recovers as you submit more good-quality gradients.

**Can I run multiple miners?**
Yes. Each miner registers with a separate wallet address and gets a separate UID.

**What data is the model trained on?**
The default adapter uses [FineWeb-Edu](https://huggingface.co/datasets/HuggingFaceFW/fineweb-edu), a high-quality educational web corpus. The Python sidecar lets you train on any dataset your script loads.

**How does the TEE scoring work?**
A validator sends your compressed gradient to an AWS Nitro Enclave. The enclave:
1. Downloads the validation batch (same deterministic seed as your training batch)
2. Runs `loss_before - loss_after` to measure how much your gradient improved the model
3. Signs the score with a hardware key whose public key is registered on-chain
4. The `score_ledger` contract verifies this signature before accepting the score

No one — not even the enclave operator — can forge a score without compromising the AWS Nitro hardware.

---

## Next Steps

- **Miners:** [Setup guide](miners/setup.md) → [Running guide](miners/running.md)
- **Validators:** [Setup guide](validators/setup.md) → [Enclave guide](validators/enclave.md)
- **Developers:** [Architecture](architecture.md) → [Security model](security.md) → [ONBOARDING.md](../ONBOARDING.md)
- **Contracts:** [Contract overview](contracts/overview.md)
- **Tokenomics:** [Incentive design](incentives.md)
