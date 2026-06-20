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
| **Run a miner** | `curl -sSL https://www.vram.network/install.sh \| bash` (closed-source signed binary; freely installable) |
| **Run a validator** | `curl -sSf https://raw.githubusercontent.com/VRAM-AI/vram-validator/main/install.sh \| bash` — test mode works on any Linux VPS; Nitro enclave requires AWS `c5.xlarge` |
| **Post a training job** | https://www.vram.network/training or `vramhub-cli job post` |
| **Try the demo** | https://www.vram.network/demotrain — upload a dataset, see Walrus integration + cost simulator + animated training |
| **Plug a new model architecture** | Implement `TrainingFrameworkAdapter` (see `crates/vramhub-adapter/src/lib.rs`); reference impls: ToyAdapter, CandleAdapter, SidecarAdapter |
| **Build on top via SDK** | Public SDK `vram-sdk` launches Q3 2026; until then email `builders@vram.ai` for early access |
| **Audit the contracts** | `contracts/sources/*.move` (MIT); v0.7 deployed at package [`0xaff18bf6…`](https://suiscan.xyz/testnet/object/0xaff18bf6286047126901610d758d8fd111c9215a6e46abc704b6a0be838badd5) |

---

## Architecture in 60 Seconds

```
┌──────────────┐   gradient (Walrus)    ┌──────────────────────┐
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
| `vramhub-miner` | Trains on assigned data batch, compresses gradient (top-K f16), uploads to Walrus (free on testnet), anchors blob ID on-chain |
| `vramhub-validator` | Downloads gradients from Walrus public aggregator, runs Nitro enclave scoring |
| Nautilus enclave | AWS Nitro TEE — scores gradient quality (loss delta), signs with hardware Ed25519 key |
| Sui contracts | PeerRegistry, ScoreLedger, RewardPool — coordination and token distribution |
| VRAMScan | Block explorer at `https://www.vram.network` — shows live windows, miner scores, run history, training guide |
| Python sidecar | `scripts/vram_trainer.py` — any HuggingFace causal LM trains via HTTP; Rust handles chain/Walrus/compression |

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

**Token:** VRAM · 9 decimals · 21M hard cap · 70 VRAM/window emission (Phase 1)  
**Explorer:** https://suiscan.xyz/testnet/object/0xaff18bf6286047126901610d758d8fd111c9215a6e46abc704b6a0be838badd5

---

## Option A — Explore the Live Network (no setup)

The easiest starting point is the live block explorer:

- **https://www.vram.network** — live training windows, miner leaderboard, scores
- **https://www.vram.network/demotrain** — simulate posting a training job (Walrus upload + cost estimator)
- **https://www.vram.network/training** — earnings calculator and GPU pricing guide
- **https://suiscan.xyz/testnet/object/0xaff18bf6286047126901610d758d8fd111c9215a6e46abc704b6a0be838badd5** — raw on-chain state

To audit the contracts locally (no wallet needed):

```bash
git clone https://github.com/VRAM-AI/vram-network.git
cd vram-network/contracts

# Run all 101 Move unit tests
sui move test
```

---

## Option B — Connect a Real Miner to Testnet (~10 minutes)

### Prerequisites

- Linux x86_64 or aarch64 machine with any NVIDIA GPU (CPU fallback works, earnings are lower)
- Python 3.8+ and pip
- A Sui wallet with testnet SUI (free from faucet)

### Step 1 — Pre-install Python dependencies

On Debian/Ubuntu systems (including RunPod), the installer's pip step may fail because system-installed packages (`blinker`, `flask`) lack RECORD files. Pre-install them first to avoid this:

```bash
pip3 install --ignore-installed \
  "transformers>=5.0.0" \
  "huggingface-hub>=1.5.0,<2.0" \
  "datasets>=2.18.0" \
  "accelerate>=0.27.0" \
  "flask>=3.0.0" \
  "numpy>=1.24.0" \
  "einops>=0.7.0"
```

Then run the installer (pip step passes because packages are already present):

```bash
curl -sSL https://www.vram.network/install.sh | bash
```

The script downloads the `vramhub-miner` binary, installs the Python sidecar, creates `~/.vramhub/.env` with all testnet contract IDs pre-filled, and creates a `~/.vramhub/start-miner.sh` launcher.

### Step 2 — Set your wallet mnemonic and verify demo mode

```bash
nano ~/.vramhub/.env
```

Set your mnemonic — **quote the value** (multi-word values must be quoted for the launcher's `source` to work):

```
VRAMHUB_WALLET_MNEMONIC="your twelve word mnemonic phrase here"
```

Also confirm these two lines are present (they should be — added by installer):

```
VRAMHUB_STORAGE_BACKEND=walrus
VRAMHUB_DEMO_MODE=true
```

> If you see `Error: Missing environment variable: VRAMHUB_AES_FALLBACK_KEY`, it means `VRAMHUB_DEMO_MODE=true` is missing from your `.env`. Add it and restart.

Get a free Sui wallet if you don't have one:
```bash
sui keytool generate ed25519
# Then fund at: https://faucet.sui.io
```

> **Security:** Keep your mnemonic private — it controls your wallet and signs all on-chain operations.

### Step 3 — Set the model to match the active training job

Check the active training job at [vram.network/training](https://www.vram.network/training). The miner's Python sidecar must load the **same model** as the posted job. Edit `~/.vramhub/.env`:

```
VRAMHUB_MODEL=google/gemma-4-E2B-it
```

**Gemma 4 requires PyTorch ≥ 2.7.0.** If your environment has an older torch (common on pre-built cloud images), you'll see:

```
ImportError: cannot import name 'AuxRequest' from 'torch.nn.attention.flex_attention'
```

Fix: upgrade torch and torchvision together from the PyTorch index:

```bash
# Check what you have:
python3 -c "import torch; print(torch.__version__)"
python3 -c "from torch.nn.attention.flex_attention import AuxRequest; print('OK')"

# Upgrade if AuxRequest is missing (adjust cu121/cu128 for your CUDA version):
pip3 install --ignore-installed torch torchvision \
  --index-url https://download.pytorch.org/whl/cu128
```

> **While upgrading:** set `VRAMHUB_MODEL=gpt2` to verify the rest of the stack works. GPT-2 trains on any torch version and needs no HuggingFace login.

For gated models like Gemma, you need a HuggingFace token with access:

```bash
# Accept the license at: https://huggingface.co/google/gemma-4-E2B-it
# Then create a token at: https://huggingface.co/settings/tokens
hf auth login
```

> **Note:** `huggingface-cli` is deprecated on newer systems — use `hf auth login` instead.

### Step 4 — Start mining

```bash
~/.vramhub/start-miner.sh
```

The miner auto-registers on first startup. You'll see your UID printed — track your score on [VRAMScan](https://www.vram.network).

**Logs and control:**
```bash
~/.vramhub/start-miner.sh --logs   # tail live logs
~/.vramhub/start-miner.sh --stop   # stop miner + sidecar
```

> Gemma (~16 GB) takes a few minutes to download on first run. The sidecar prints `VRAM sidecar listening on 0.0.0.0:17070` when ready.

---

## Option C1 — Run a Validator in Test Mode (Linux, no AWS required)

Test mode lets any Linux VPS participate as a validator on testnet. Scoring is simulated (no Nitro enclave), but the full chain flow works — scores are submitted on-chain and miners get rated. VRAMScan labels these validators as **Simulated**.

### Step 1 — Install the validator binary

```bash
curl -sSf https://raw.githubusercontent.com/VRAM-AI/vram-validator/main/install.sh | bash
```

This installs `vram-validator` and `vram-cli` into `/usr/local/bin`.

### Step 2 — Configure your environment

```bash
# Download the example config
curl -o ~/.env https://raw.githubusercontent.com/VRAM-AI/vram-validator/main/.env.example

# Edit it — the only required field is your wallet mnemonic
nano ~/.env
# Set: VRAMHUB_WALLET_MNEMONIC="your twelve word mnemonic here"
```

> **New wallet?** You can generate one with `sui keytool generate ed25519`.

> **Already registered as a miner?** A Sui address can only be registered once per role. If your wallet is already registered as a miner (check with `vram-cli status`), create a new wallet for the validator.

### Step 3 — Fund your wallet

You need ~12 SUI for the validator stake (10 SUI) plus gas. Get it free from the faucet:

```bash
# Replace with your wallet address (shown by: vram-cli status)
ADDR="0xYOUR_WALLET_ADDRESS"
for i in $(seq 1 5); do
  curl -s -X POST https://faucet.testnet.sui.io/v1/gas \
    -H 'Content-Type: application/json' \
    -d "{\"FixedAmountRequest\":{\"recipient\":\"${ADDR}\"}}" && sleep 3
done
```

### Step 4 — Register on-chain

```bash
source ~/.env
vram-cli register-validator
# Prints: Registered as validator UID=<N>
```

Set your UID in `~/.env`:
```bash
# Add these lines to ~/.env:
VRAMHUB_VALIDATOR_UID=<N>         # UID from register-validator output
SLCL_VALIDATOR_UID=<N>
SLCL_TEST_MODE=true
SLCL_NITRO_ENCLAVE=false
SLCL_SKIP_SEAL=true
```

### Step 5 — Start the validator

```bash
source ~/.env && vram-validator
# Expected output:
#   mode=Simulated  window=<N>  peers=<N>  ...
```

> Test mode is **rejected by mainnet contracts** — testnet only. For production, see Option C2 below.

---

## Option C2 — Run a Validator with Nitro Enclave (requires AWS, ~$36/month spot)

Validators need a Nitro Enclave-capable EC2 instance (`c5.xlarge` minimum, spot ~$0.05/hr).

Follow Option C1 Steps 1–4 first, then:

```bash
# On the c5.xlarge EC2 (--enclave-options Enabled=true)
# Build the enclave image — prints PCR0, PCR1, PCR2
./scripts/build-enclave.sh

# Register PCR values on-chain (admin/deployer wallet)
sui client call \
  --package 0xaff18bf6286047126901610d758d8fd111c9215a6e46abc704b6a0be838badd5 \
  --module hparams \
  --function update_pcrs \
  --args <hparams_id> <pcr0> <pcr1> <pcr2>

# In ~/.env, change:
# SLCL_NITRO_ENCLAVE=true
# SLCL_TEST_MODE=false
# SLCL_SKIP_SEAL=false

# Start the validator daemon (Nitro mode)
source ~/.env && vram-validator
```

See **[docs/validators/setup.md](docs/validators/setup.md)** for the full enclave build guide.

---

## Tokenomics (VRAM Token)

| Parameter | Value |
|-----------|-------|
| Hard cap | 21,000,000 VRAM |
| Decimals | 9 |
| Mining pool (50%) | 10,500,000 VRAM |
| TGE pre-mint (50%) | 10,500,000 VRAM |
| Window duration | 10 minutes |
| Phase 1 emission | 70 VRAM/window (0 → 7M mining tokens) |
| Phase 2 emission | 35 VRAM/window (7M → 10.5M) |

**TGE pre-mint breakdown:**

| Allocation | % | VRAM | Vesting |
|------------|---|------|---------|
| Treasury | 25% | 5,250,000 | 6m cliff, 48m linear |
| Strategic Investors | 5% | 1,050,000 | TGE lock-up + linear vest (per SAFT terms) |
| Team | 8% | 1,680,000 | 12m cliff, 36m linear |
| Liquidity | 7% | 1,470,000 | 100% unlocked at TGE |
| Airdrop | 5% | 1,050,000 | Instant at TGE (converts from testnet contribution points) |

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
  vramhub-comms/       Walrus storage client, dataset loader (FineWeb-Edu shards)
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

VRAMScan          Next.js block explorer — https://www.vram.network
  /miners               miner leaderboard with OpenSkill weights
  /validators           validator list and stake
  /windows              historical window data
  /wallet/[address]     user quest tracking and earnings
  /training             earnings calculator, GPU pricing comparison, quick-start
  /join                 step-by-step onboarding for new miners and validators
  /demotrain            training job demo (Walrus upload + cost simulator)

docs/
  getting-started.md    non-technical introduction (start here)
  architecture.md       full system design and trust model
  security.md           security model, known limitations, threat analysis
  incentives.md         OpenSkill scoring and tokenomics math
  miners/setup.md       miner setup guide (registration, wallet, Walrus)
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
| VRAMScan explorer + /training page | ✅ Live at www.vram.network |
| Seal IBE credential privacy | ✅ Configured (testnet key servers) |
| Validator (test mode) | ✅ Live — `vram-cli register-validator` + simulated scoring |
| Validator (Nitro enclave) | ⏳ Needs `c5.xlarge` + enclave build (~$36/mo spot) |
| VRAM token payouts | ⏳ Unblocked once first Nitro validator is up |
| Nitro root CA validation | ⏳ Ed25519 sig verification done; full cert chain pending |
| GPU marketplace / rental | 🗓 Planned — rent GPU capacity without running a full miner |

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for the development workflow, testing instructions, and how to add a training adapter.

Security issues: see [SECURITY.md](SECURITY.md). Do not open public GitHub issues for vulnerabilities.
