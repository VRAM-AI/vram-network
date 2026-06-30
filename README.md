# Vram Network

[![Sui Testnet](https://img.shields.io/badge/sui-testnet-72D900)](https://suiscan.xyz/testnet/object/0xaff18bf6286047126901610d758d8fd111c9215a6e46abc704b6a0be838badd5)
[![Move tests](https://img.shields.io/badge/move%20tests-101%2F101-72D900)](contracts/tests/)
[![License](https://img.shields.io/badge/contracts-MIT-72D900)](LICENSE)
[![Apache 2.0](https://img.shields.io/badge/sdk-Apache%202.0-72D900)](LICENSE)

**Trustless decentralised machine learning on Sui.** GPU miners train models; AWS Nitro Enclave validators score every gradient and sign the proof on-chain; rewards settle in **VRAM** every 10 minutes — no trusted coordinator, no whitelist.

[**Block explorer →**](https://www.vram.network) · [**Live demo**](https://www.vram.network/demotrain) · [**Sui Explorer**](https://suiscan.xyz/testnet/object/0xaff18bf6286047126901610d758d8fd111c9215a6e46abc704b6a0be838badd5)

---

## What's in this repo

This is the open-source SDK for Vram Network — the contracts, on-chain client, adapter framework, operator CLI, and Python sidecar trainer. Everything here is open and auditable.

The miner and validator **binaries** are closed-source and distributed separately (see below). This repo contains the protocol they speak.

| Component | Path | What it is |
|-----------|------|------------|
| Smart contracts | `contracts/sources/` | 13 Sui Move modules — PeerRegistry, ScoreLedger, RewardDistributor, TrainingJobBoard, and more |
| Core types + OpenSkill | `vramhub-core/` | Shared types, Bayesian rating system, error types — no I/O |
| Sui RPC client | `vramhub-chain/` | All on-chain calls: register, submit scores, claim rewards, post training jobs |
| Training adapters | `vramhub-adapter/` | Pluggable adapter trait + impls: Candle nano-GPT, Python sidecar, custom |
| Operator CLI | `vramhub-cli/` | `vram-cli register-miner`, `register-validator`, `status`, `scores` |
| Python sidecar | `python/` | Run any HuggingFace model as a VRAM miner via HTTP |
| Docs | `docs/` | Architecture, incentives, contract reference, env-var guide |

---

## Run a miner

```bash
curl -sSL https://www.vram.network/install.sh | bash
```

Set your wallet mnemonic in `~/.vramhub/.env` and run `vram-miner`. Full guide: [ONBOARDING.md — Option B](ONBOARDING.md#option-b--connect-a-real-miner-to-testnet-10-minutes).

| | |
|--|--|
| Network | Sui Testnet |
| Reward pool | 6,000,000 VRAM ready to distribute |
| Emission | 70 VRAM / 10-minute window |
| Min stake | 1 SUI (refundable) |

---

## Run a validator

The validator installer lives at **[VRAM-AI/vram-validator](https://github.com/VRAM-AI/vram-validator)**.

```bash
curl -sSf https://raw.githubusercontent.com/VRAM-AI/vram-validator/main/install.sh | bash
```

Installs `vram-validator` + `vram-cli`. Full setup guide: [ONBOARDING.md — Option C1/C2](ONBOARDING.md#option-c1--run-a-validator-in-test-mode-linux-no-aws-required).

- **Test mode** — any Linux VPS, simulated scoring, no AWS required
- **Nitro enclave** — AWS `c5.xlarge`, hardware-attested scoring (~$36/mo spot)

---

## Train a model

Post a training job on [vram.network/training](https://www.vram.network/training). The reference dataset (Sui Move / DeepBook instruction pairs for `google/gemma-4-E2B-it`) is at **[VRAM-AI/training](https://github.com/VRAM-AI/training)**.

```bash
# Start the Python sidecar on your GPU with the Move dataset
python python/vram_trainer.py \
  --model google/gemma-4-E2B-it \
  --dataset https://raw.githubusercontent.com/VRAM-AI/training/main/data/train_clean.jsonl \
  --device cuda --dtype bfloat16 --batch 2 --seqlen 2048
```

---

## v0.7 testnet contracts

| Object | ID |
|--------|-----|
| Package | `0xaff18bf6286047126901610d758d8fd111c9215a6e46abc704b6a0be838badd5` |
| PeerRegistry | `0x50a9982f6a3d6c1e6674f0fb4fa8b985007dbe19fc797abc691579be1f6493df` |
| ValidatorRegistry | `0x438d0ce63d40210d4e621cca6aaaf5d5438adaa54dfd71383fe41a82692a2561` |
| EnclaveRegistry | `0x442b82e471c1ee4577ea1f2168deb1f0b04fcc861ab79edb4b9c7d7738bf7f9f` |
| ScoreLedger | `0x0d2594727abeb45a13763baf8801ae765fbe41d147b28916ca78a0d08f73223a` |
| RoundState | `0xc1f18dc92629907641bc3176449af39738d2d8a93b4ad6b22548f4aed91d2611` |
| Hparams | `0x18b884530033f9b3e449b898c540ee5d3a25c4cab0abcf4843ef8e86e12adbfc` |
| RewardPool | `0x576ebeb78449ad46ef70dc3c5ca4e38d178846610bd7cf9f0764ae2f1dc0fe93` |
| TrainingJobBoard | `0xb481254350087569f904fe6fc45c337c0905651040791e532e0f044b9fc7474c` |

Chain-id `4c78adac` · Deployer [`0xb7aaeb31…c772c3`](https://suiscan.xyz/testnet/account/0xb7aaeb31d576814e1b268a43033feccac19a2905a652ad3b42fb5efeb1c772c3)

```bash
# Verify contracts locally — no wallet needed
git clone https://github.com/VRAM-AI/vram-network.git
cd vram-network/contracts && sui move test
```

---

## How to contribute

The quickest entry points depending on what you want to do:

### Audit / improve the contracts
All 13 Move modules are in `contracts/sources/`. Run `sui move test` — 101 tests, all must pass. Good first areas: `training_jobs.move` (job lifecycle), `score_ledger.move` (signature verification), `reward_distributor.move` (halving math).

### Add a training adapter
Implement `TrainingFrameworkAdapter` in `vramhub-adapter/src/`. The trait has five methods: `train_step`, `forward_loss`, `compress_gradient`, `load_checkpoint`, `save_checkpoint`. See `vramhub-adapter/src/sidecar.rs` for the simplest reference (~150 lines — it just calls a local HTTP server). See [CONTRIBUTING.md](CONTRIBUTING.md) for the full walkthrough.

### Improve the on-chain client
`vramhub-chain/src/` has one file per object type. If a new contract method is added, the corresponding Rust function goes here. The client is pure async Rust with no hidden state — easy to extend.

### Write a Python sidecar for a new model family
`python/vram_trainer.py` supports any `AutoModelForCausalLM` model via `--model`. To add a new dataset format or training objective, extend `_get_batch()`. See [python/README.md](python/README.md) if it exists, otherwise read the docstring at the top of the file.

### Fix docs
`docs/` is CC BY 4.0. If something is wrong or out of date, open a PR directly — no issue needed for doc fixes.

### Run a node and report issues
The fastest way to find real bugs is to actually run a miner or validator and report what breaks. See [ONBOARDING.md](ONBOARDING.md).

---

## Architecture

```
   GPU Miner ──── compressed gradient ───→  Walrus (Sui)
                                                 │
                                                 ▼
                                        Validator (AWS Nitro Enclave)
                                                 │
                                         attested loss-delta
                                                 ▼
                                        Sui ScoreLedger
                                                 │
                                      Bayesian OpenSkill update
                                                 ▼
                                        RewardDistributor
                                                 │
                                         VRAM per window
                                                 ▼
                                           Miner wallet
```

Full threat model and trust assumptions: [docs/architecture.md](docs/architecture.md)

---

## Tokenomics

- **Hard cap**: 21,000,000 VRAM
- **Mining pool (50%)**: emitted per-window, supply-based halving — Phase 1: 70 VRAM/window → Phase 2: 35 VRAM/window
- **TGE pre-mint (50%)**: Treasury 25% (6m cliff, 48m vest) · Investors 5% (SAFT lock-up) · Team 8% (12m cliff, 36m vest) · Liquidity 7% (instant) · Airdrop 5% (converts from testnet points)
- **Validator bonding**: 2,100 → 21,000 VRAM burned per slot (4 tiers, max 500 validators)

Full model: [docs/incentives.md](docs/incentives.md)

---

## Roadmap

| Feature | Status |
|---------|--------|
| Miner binary (testnet) | ✅ Live |
| Validator — test mode | ✅ Live |
| Validator — Nitro enclave (mainnet) | ⏳ In progress |
| VRAM token payouts | ⏳ Pending first Nitro validator |
| vram-sdk public release | 🗓 Q3 2026 |
| GPU marketplace / rental | 🗓 Planned |

---

## License

- **Contracts** (`contracts/`) — MIT
- **SDK crates** (`vramhub-*/`) — Apache 2.0
- **Validator installer** ([VRAM-AI/vram-validator](https://github.com/VRAM-AI/vram-validator)) — MIT
- **Miner / Validator binaries — proprietary; installable via the official installer scripts (see ONBOARDING.md)

Security issues: email security@vram.ai — see [SECURITY.md](SECURITY.md). Do not open public issues for vulnerabilities.

---

**Team**: team@vram.ai · **Builders**: builders@vram.ai · © 2024-2026 VRAM AI Limited
