# Miner Setup

This guide covers setting up a VRAM HUB miner node. Miners provide GPU compute, train on assigned data batches, and upload compressed gradients to Cloudflare R2.

## Prerequisites

| Requirement | Minimum |
|-------------|---------|
| GPU | Any NVIDIA (CUDA), Apple Silicon (Metal), or CPU-only |
| RAM | 8 GB (16 GB recommended for larger models) |
| Disk | 10 GB (dataset shards cached at `~/.slcl/fineweb/`) |
| Network | 10 Mbps upload |
| Rust | 1.80+ |
| OS | Linux / macOS / Windows (WSL2) |

You also need:
- A **Cloudflare R2** bucket (free tier is sufficient)
- A **Sui wallet** with at least 1 SUI for stake (testnet SUI is free)

## Step 1: Install Dependencies

```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Sui CLI (download prebuilt from releases)
# https://github.com/MystenLabs/sui/releases — download the latest testnet binary
tar -xzf sui-*.tgz
cp sui ~/.cargo/bin/
```

For CUDA support you also need:
- [CUDA Toolkit ≥ 11.8](https://developer.nvidia.com/cuda-downloads)
- cuDNN (matching CUDA version)

## Step 2: Clone and Build

```bash
git clone https://github.com/VRAM-AI/VRAM-HUB.git
cd VRAM-HUB

# CPU-only build (works everywhere)
cargo build --release --bin vramhub-miner

# NVIDIA GPU build
cargo build --release --bin vramhub-miner --features cuda

# Apple Silicon
cargo build --release --bin vramhub-miner --features metal

# Python sidecar (bring your own model)
cargo build --release --bin vramhub-miner --features sidecar
```

The first build takes several minutes (compiling Sui SDK + Candle).

## Step 3: Set Up Cloudflare R2

1. Go to [dash.cloudflare.com](https://dash.cloudflare.com) → R2 → **Create bucket** (e.g. `vram-gradients-yourname`)
2. R2 → **Manage API tokens** → Create token with **Object Read & Write** on your bucket
3. Note your:
   - Account ID (visible in R2 dashboard URL)
   - Bucket name
   - Access Key ID
   - Secret Access Key

## Step 4: Configure Environment

```bash
cp .env.example .env
```

Edit `.env` — fill in your wallet mnemonic and R2 credentials:

```bash
# Required: your wallet mnemonic
VRAMHUB_WALLET_MNEMONIC="word1 word2 ... word12"

# R2 credentials
VRAMHUB_R2_ACCOUNT_ID=your_cloudflare_account_id
VRAMHUB_R2_BUCKET_NAME=vram-gradients-yourname
VRAMHUB_R2_ACCESS_KEY_ID=your_r2_access_key
VRAMHUB_R2_SECRET_ACCESS_KEY=your_r2_secret_key

# Optional: override GPU device (default: auto-detect CUDA > Metal > CPU)
# VRAMHUB_DEVICE=cuda:0
```

All other values (RPC URL, contract IDs, Seal key servers) are pre-filled for testnet.

`VRAMHUB_MINER_UID` is **optional** — the daemon auto-registers on first startup and saves your UID to `.vramhub-uid`.

## Step 5: Get Testnet SUI

```bash
sui client new-env --alias testnet --rpc https://fullnode.testnet.sui.io:443
sui client switch --env testnet
sui client new-address ed25519   # note the address printed

# Claim free testnet SUI
# Open in browser: https://faucet.sui.io
# Paste your address from the step above
```

Verify:
```bash
sui client balance
# Should show ≥ 1 SUI
```

## Step 6: Register and Run

The miner handles registration automatically on first startup:

```bash
source .env

# CPU-only
cargo run --release --bin vramhub-miner --features candle

# NVIDIA GPU
cargo run --release --bin vramhub-miner --features cuda
```

On first run:
```
INFO vramhub_miner: No VRAMHUB_MINER_UID set — looking up on-chain...
INFO vramhub_miner: Registering as miner with 1 SUI stake
INFO vramhub_miner: Registered as miner uid=3
INFO vramhub_miner: Saved uid=3 to .vramhub-uid
```

Alternatively, register manually before starting:

```bash
cargo run --release --bin vramhub-cli -- register-miner \
  --bucket your-r2-bucket \
  --account-id your-cloudflare-account-id
```

## Step 7: Verify Registration

```bash
cargo run --release --bin vramhub-cli -- status
```

Or open VRAMScan at http://localhost:4322/miners.

## Python Sidecar (Optional)

To train with a custom PyTorch model instead of the built-in nano-GPT:

```bash
# Install Python dependencies
pip install -r scripts/requirements.txt

# Start the trainer (Terminal 1)
python scripts/vram_trainer.py --model gpt2 --device cuda --port 17070

# Start the miner pointed at the sidecar (Terminal 2)
VRAMHUB_SIDECAR_URL=http://127.0.0.1:17070 \
  cargo run --release --bin vramhub-miner --features sidecar
```

The sidecar supports any HuggingFace causal LM: `gpt2`, `mistralai/Mistral-7B-v0.1`, `meta-llama/Meta-Llama-3-8B`, etc.

## Next: Running the Miner

See [Running a Miner](running.md) for training adapters, log output, monitoring, and systemd setup.
