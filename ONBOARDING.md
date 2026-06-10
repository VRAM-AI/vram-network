# Vram Network — Builder Onboarding

Welcome. This document explains what Vram Network is, what's live on testnet today, and how to get started **building on top of it** in under 30 minutes.

> **Internal engineers**: see `internal/PROJECT_STATE.md` for current operational state, mainnet blockers, and the road-to-testnet checklist. This file is the **public-facing** quick start.

For a non-technical introduction, see **[docs/getting-started.md](docs/getting-started.md)**.

---

## What Is Vram Network?

Vram Network is a **decentralised LLM training coordination protocol** on Sui.

Instead of one company training a large language model on a centralised cluster, Vram distributes the work across independent GPU miners worldwide. Validators verify the work inside a Trusted Execution Environment (TEE). Contributors earn **VRAM tokens** proportional to how much their gradients actually improved the model.

The key insight: we don't trust miners to self-report results. A Nitro enclave measures the actual loss improvement each gradient produces, signs the score with a hardware key, and posts it on-chain. No one — not even the enclave operator — can fake a score.

**Analogy:** Think Bittensor (TAO) but the "proof of work" is gradient quality verified by a TEE instead of validator consensus.

---

## What you can do today

| Goal | How |
|---|---|
| **Run a miner** | `curl -sSL https://install.vram.ai/miner \| sh` (closed-source signed binary; freely installable) |
| **Run a validator** | Requires AWS Nitro Enclave host; see `scripts/install-validator.sh` and `docs/validators/setup.md` |
| **Post a training job** | https://vramscan.io/training or `vramhub-cli job post` |
| **Try the demo** | https://vramscan.io/demotrain — upload a dataset, see Walrus integration + cost simulator + animated training |
| **Plug a new model architecture** | Implement `TrainingFrameworkAdapter` (see `crates/vramhub-adapter/src/lib.rs`); reference impls: ToyAdapter, CandleAdapter, SidecarAdapter |
| **Build on top via SDK** | Public SDK `vram-sdk` launches Q3 2026; until then email `builders@vram.ai` for early access |
| **Audit the contracts** | `contracts/sources/*.move` (MIT); v0.7 deployed at package [`0xaff18bf6…`](https://suiscan.xyz/testnet/object/0xaff18bf6286047126901610d758d8fd111c9215a6e46abc704b6a0be838badd5) |

---

## Architecture in 60 Seconds

```
┌──────────────┐     gradient (R2)      ┌──────────────────────┐
│    Miner     │ ───────────────────▶  │  Nitro Enclave (TEE) │
│  (any GPU)   │                        │  - downloads gradient │
└──────────────┘                        │  - scores loss delta  │
                                        │  - signs with Ed25519 │
┌──────────────┐    scored window       └──────────┬───────────┘
│  Validator   │ ◀──────────────────────────────── │
│  (AWS EC2)   │                                    │ signed score
└──────┬───────┘                                    ▼
       │ submit_scores()              ┌─────────────────────────┐
       └─────────────────────────────▶   Sui Testnet           │
                                      │  - ScoreLedger          │
                                      │  - OpenSkill ratings    │
                                      │  - VRAM token rewards   │
                                      └─────────────────────────┘
```

**Key components:**

| Component | What it does |
|-----------|-------------|
| `vramhub-miner` | Trains on assigned data batch, compresses gradient (top-K f16), uploads to R2, anchors checkpoint hash on-chain |
| `vramhub-validator` | Decrypts miner R2 credentials via Seal IBE, downloads gradients, runs Nitro enclave scoring |
| Nautilus enclave | AWS Nitro TEE — scores gradient quality (loss delta), signs with hardware Ed25519 key |
| Sui contracts | PeerRegistry, ScoreLedger, RewardPool — coordination and token distribution |
| VRAMScan | Block explorer at `http://localhost:4322` — shows live windows, miner scores, run history, training guide |
| Python sidecar | `scripts/vram_trainer.py` — any HuggingFace causal LM trains via HTTP; Rust handles chain/R2/compression |

---

## What's Live on Testnet Today

Everything is deployed and funded. No setup required on the contract side.

| Object | ID |
|--------|-----|
| Package | `0xaff18bf6286047126901610d758d8fd111c9215a6e46abc704b6a0be838badd5` |
| PeerRegistry | `0x50a9982f6a3d6c1e6674f0fb4fa8b985007dbe19fc797abc691579be1f6493df` |
| ValidatorRegistry | `0x438d0ce63d40210d4e621cca6aaaf5d5438adaa54dfd71383fe41a82692a2561` |
| ScoreLedger | `0x0d2594727abeb45a13763baf8801ae765fbe41d147b28916ca78a0d08f73223a` |
| RoundState | `0xc1f18dc92629907641bc3176449af39738d2d8a93b4ad6b22548f4aed91d2611` |
| RewardPool | `0x576ebeb78449ad46ef70dc3c5ca4e38d178846610bd7cf9f0764ae2f1dc0fe93` |

**Token:** VRAM · 9 decimals · 500M hard cap · 1,200 VRAM/window emission  
**Explorer:** https://suiscan.xyz/testnet/object/0xaff18bf6286047126901610d758d8fd111c9215a6e46abc704b6a0be838badd5

---

## Option A — Test Locally (no wallet, no AWS, 5 minutes)

This runs a full simulation: 6 miners + 3 validators, a toy bigram LLM, and live scoring — all in-process. No wallet, no GPU, no cloud accounts needed.

```bash
git clone https://github.com/VRAM-AI/VRAM-HUB.git
cd VRAM-HUB

# Terminal 1 — run the demo
cargo run -p vramhub-local-demo

# Terminal 2 — run the block explorer
cd vramscan
npm install
npm run dev
```

Open **http://localhost:4322** — you will see:
- Live training windows ticking up
- Miner weight bars updating every 10 windows
- `/runs` page showing run history with score progression charts

The demo writes a persistent run log to `.vramhub-runs/run_history.jsonl`. It survives restarts so you can compare runs over time.

**What to look for:**
- Miners start with equal weights, then diverge as OpenSkill ratings converge
- Top miners get exponentially more weight (squared ordinal weighting)
- Window clock advances every few seconds (sped up for demo)

---

## Option B — Connect a Real Miner to Testnet (30 minutes)

### Prerequisites

- Linux machine with any NVIDIA GPU (or Apple Silicon for Metal)
- Rust 1.80+ (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
- A free [Cloudflare](https://dash.cloudflare.com) account for R2 storage

### Step 1 — Get testnet SUI

```bash
# Install Sui CLI — pick the right binary for your OS at:
# https://github.com/MystenLabs/sui/releases

# Create a wallet and connect to testnet
sui client new-env --alias testnet --rpc https://fullnode.testnet.sui.io:443
sui client switch --env testnet
sui client new-address ed25519   # note the address printed

# Fund it (paste your address at):
# https://faucet.sui.io
```

### Step 2 — Set up Cloudflare R2

1. https://dash.cloudflare.com → **R2** → **Create bucket** (e.g. `vram-miner-yourname`)
2. R2 → **Manage API tokens** → Create token with **Object Read & Write** on your bucket
3. Note: Account ID, Bucket name, Access Key ID, Secret Access Key

### Step 3 — Configure and run

```bash
git clone https://github.com/VRAM-AI/VRAM-HUB.git
cd VRAM-HUB
cargo build --workspace --release

cp .env.example .env
```

Edit `.env` — the only fields you need to change:

```bash
VRAMHUB_WALLET_MNEMONIC="your twelve word mnemonic phrase here"
VRAMHUB_R2_ACCOUNT_ID=your-cloudflare-account-id
VRAMHUB_R2_BUCKET_NAME=vram-miner-yourname
VRAMHUB_R2_ACCESS_KEY_ID=your-r2-key-id
VRAMHUB_R2_SECRET_ACCESS_KEY=your-r2-secret
```

> **Security:** Never commit `.env` to git. The `.gitignore` already excludes it.  
> Keep your mnemonic private — it controls your wallet and signs all on-chain operations.

Everything else (package ID, object IDs, Seal key servers) is pre-filled.

```bash
# Python sidecar — recommended for testnet (any HuggingFace model)
# Terminal 1:
pip install -r scripts/requirements.txt
python scripts/vram_trainer.py --model gpt2 --device cuda   # or --device cpu

# Terminal 2:
RUST_LOG=info VRAMHUB_SIDECAR_URL=http://127.0.0.1:7070 \
  cargo run --release --bin vramhub-miner --features sidecar
```

> **Note:** The `candle`, `cuda`, and `metal` feature flags exist but the Candle adapter is a stub pending [bounty #15](https://github.com/VRAM-AI/VRAM-HUB/issues/15). Use the Python sidecar for real training today.

The miner auto-registers on first startup (saves UID to `.vramhub-uid`) and starts training. You'll see your UID printed on startup — use it to track your scores on VRAMScan.

### Option B2 — Use your own PyTorch model (Python sidecar)

The sidecar lets any HuggingFace causal LM participate as a miner. Rust handles chain, R2, and compression; Python handles the forward/backward pass.

```bash
# Terminal 1 — start the Python sidecar
pip install -r scripts/requirements.txt
python scripts/vram_trainer.py --model gpt2 --device cuda

# Terminal 2 — start the miner pointing at the sidecar
VRAMHUB_SIDECAR_URL=http://127.0.0.1:7070 \
  cargo run --release --bin vramhub-miner --features sidecar
```

Supported models: any `AutoModelForCausalLM`-compatible HuggingFace model.  
The sidecar defaults to `gpt2` (124M). Pass `--model mistralai/Mistral-7B-v0.1` etc. for larger models.

---

## Option C1 — Run a Validator in Test Mode (Linux, no AWS required)

Test mode lets any Linux VPS participate as a validator on testnet. Scoring is simulated (no Nitro enclave), but the full chain flow works — scores are submitted on-chain and miners get rated. VRAMScan labels these validators as **Simulated**.

```bash
# Copy and fill in your .env (wallet mnemonic + R2 credentials)
cp .env.example .env

# Run validator — simulated scoring, no AWS hardware required
RUST_LOG=info VRAMHUB_TEST_MODE=true \
  cargo run --release --bin vramhub-validator
```

> Test mode is **rejected by mainnet contracts** — it is testnet-only. To go production, see Option C2 below.

---

## Option C2 — Run a Validator with Nitro Enclave (requires AWS, ~$36/month spot)

Validators need a Nitro Enclave-capable EC2 instance (`c5.xlarge` minimum, spot ~$0.05/hr).

> **Note:** The enclave build is a one-time step. Once PCR values are registered on-chain, the validator binary handles everything else automatically.

See **[docs/validators/setup.md](docs/validators/setup.md)** for the full guide. The short version:

```bash
# On a c5.xlarge with --enclave-options Enabled=true
./scripts/build-enclave.sh          # prints PCR0, PCR1, PCR2

# Register the PCR values on-chain
sui client call \
  --package 0xaff18bf6... \
  --module hparams \
  --function update_pcrs \
  --args <hparams_object_id> <pcr0> <pcr1> <pcr2>

# Run the validator daemon
cargo run --release --bin vramhub-validator
```

---

## Tokenomics (VRAM Token)

| Parameter | Value |
|-----------|-------|
| Hard cap | 500,000,000 VRAM |
| Decimals | 9 |
| Window duration | 10 minutes |
| Genesis emission | 1,200 VRAM/window |
| Halving | Every 4 years (governance) |
| Year 1 total emission | ~63M VRAM (~12.6% of supply) |

**Distribution per window:**

| Recipient | v0.4 (current) | v0.5+ |
|-----------|---------------|-------|
| Miners | 100% | 72% |
| Validators | — | 18% |
| Treasury | — | 10% |

Miner rewards are proportional to each miner's OpenSkill normalized weight (see [docs/incentives.md](docs/incentives.md)).

---

## Codebase Map

```
contracts/          Sui Move contracts (deployed to testnet)
  sources/
    peer_registry.move       miner/validator registration + Seal IBE credentials
    score_ledger.move        on-chain OpenSkill scores (Ed25519 signature verification)
    reward_distributor.move  VRAM token emission per window
    vram_token.move          VRAM coin (OTW, TreasuryCap held by deployer)
    hparams.move             governance parameters (batch size, compression, PCRs)
    seal_policy.move         Seal IBE access control (seal_approve entry point)

crates/
  vramhub-core/        shared types, OpenSkill impl, constants — no I/O
  vramhub-chain/       Sui RPC client — all on-chain calls (register, score, reward)
  vramhub-comms/       Cloudflare R2 client, dataset loader (FineWeb-Edu shards)
  vramhub-seal/        Seal IBE client — encrypt/decrypt R2 credentials
  vramhub-adapter/     pluggable training adapters
    src/candle_gpt.rs     nano-GPT 10M transformer (pure Rust, Candle framework)
    src/sidecar.rs        Python sidecar HTTP adapter
    src/training.rs       top-K f16 gradient compression (DCT domain)
    src/candle.rs         simple MLP adapter (baseline)
  vramhub-miner/       miner daemon binary
    src/miner.rs          main training loop (window → train → compress → upload → anchor)
    src/config.rs         env-var config loading
    src/training.rs       gradient compression helpers
  vramhub-validator/   validator daemon binary
    src/validator.rs      main validation loop (download → enclave → submit)
    src/attestation.rs    Nitro attestation document verification
    src/fast_eval.rs      synchronous forward-pass scoring
  vramhub-nautilus/    Nitro Enclave server (TEE scoring binary)
  vramhub-aggregator/  gradient aggregation, checkpoint building
  vramhub-local-demo/  in-process simulation — no wallet or GPU needed
  vramhub-cli/         operator CLI (register-miner, register-validator, status, scores)

scripts/
  vram_trainer.py   Python sidecar — any HuggingFace causal LM
  requirements.txt  Python dependencies (torch, transformers, flask)
  build-enclave.sh  Nitro EIF build script (prints PCR0/PCR1/PCR2)

vramscan/           Next.js block explorer (http://localhost:4322)
  app/
    page.tsx              network overview (live window, peer counts, emission)
    /miners               miner leaderboard with OpenSkill weights
    /validators           validator list and stake
    /windows              historical window data
    /window/[id]          single window detail
    /wallet/[address]     user quest tracking and earnings
    /runs                 local demo run history
    /training             earnings calculator, GPU pricing comparison, quick-start
    /join                 step-by-step onboarding for new miners and validators
    /docs/[[...slug]]     protocol docs rendered from /docs directory
  lib/
    api-real.ts           live Sui RPC client (all blockchain reads)
    tokenomics.ts         VRAM emission schedule math
    types.ts              TypeScript interfaces

docs/
  getting-started.md    non-technical introduction (start here)
  architecture.md       full system design and trust model
  security.md           security model, known limitations, threat analysis
  incentives.md         OpenSkill scoring and tokenomics math
  miners/setup.md       miner setup guide (registration, wallet, R2)
  miners/running.md     all training adapters, gradient compression
  validators/           validator + enclave guides
  reference/env-vars.md environment variable reference (all VRAMHUB_* variables)

paper/
  vram-hub-paper.pdf    academic paper (PDF)
  vram-hub.tex          LaTeX source
  vram-hub-paper.md     Markdown version
```

---

## Status

| Area | Status |
|------|--------|
| Contracts on testnet | ✅ Live |
| VRAM token + RewardPool funded | ✅ 6M VRAM deposited |
| Miners — auto-registration | ✅ Ready |
| Miners — Candle nano-GPT (CPU) | ✅ Ready |
| Miners — CUDA / Metal training | ✅ Ready |
| Miners — Python sidecar | ✅ Ready |
| Top-K f16 gradient compression | ✅ Ready |
| VRAMScan explorer + /training page | ✅ Running locally |
| Seal IBE credential privacy | ✅ Configured (testnet key servers) |
| Validator scoring | ⏳ Needs Nitro enclave (~$36/mo spot) |
| VRAM token payouts | ⏳ Unblocked once first validator is up |
| Nitro root CA validation | ⏳ Ed25519 sig verification done; full cert chain pending |

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for the development workflow, testing instructions, and how to add a training adapter.

Security issues: see [SECURITY.md](SECURITY.md). Do not open public GitHub issues for vulnerabilities.
