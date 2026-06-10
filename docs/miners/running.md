# Running a Miner

## Start the Miner Daemon

```bash
source .env
cargo run --release --bin vramhub-miner
```

The miner will:
1. Auto-detect or auto-register your UID (see below)
2. Fetch current hyperparameters from chain
3. Enter the window loop:
   - Load latest checkpoint from R2
   - Train on assigned data batch
   - Compress gradient (top-K f16)
   - Upload gradient to R2
   - Anchor checkpoint hash on-chain

## Auto-Registration

`VRAMHUB_MINER_UID` is **optional**. On first startup the miner resolves your UID using this priority chain:

1. `VRAMHUB_MINER_UID` env var (if set)
2. `.vramhub-uid` file in the working directory (written on first registration)
3. On-chain lookup by wallet address
4. Auto-register with 1 SUI stake → saves UID to `.vramhub-uid`

You can also register manually before starting the daemon:

```bash
cargo run --release --bin vramhub-cli -- register-miner \
  --bucket your-r2-bucket \
  --account-id your-cloudflare-account-id
```

## Training Adapters

Select your training backend at compile time via Cargo feature flags.
Priority order: `templar > sidecar > cuda > metal > candle > stub`.

### Nano-GPT — Pure Rust (CPU)

```bash
cargo run --release --bin vramhub-miner --features candle
```

A trainable 10M-parameter nano-GPT transformer implemented in pure Rust using
the [Candle](https://github.com/huggingface/candle) ML framework.
Works on any machine — no GPU, no CUDA toolkit required.

- Architecture: 6-layer, 384-dim, 6-head, 256 context
- Tokenizer: cl100k_base (100k vocab, same as GPT-4)
- Dataset: FineWeb-edu shards, deterministically assigned by `(uid, window)`

### Nano-GPT — NVIDIA GPU (CUDA)

```bash
# Requires CUDA toolkit ≥ 11.8 and cuDNN installed
cargo run --release --bin vramhub-miner --features cuda
```

Same nano-GPT model, running on your NVIDIA GPU. Typically 10–50× faster than CPU.
The device defaults to `cuda:0`; override with `VRAMHUB_DEVICE=cuda:1`.

### Nano-GPT — Apple Silicon (Metal)

```bash
cargo run --release --bin vramhub-miner --features metal
```

Same nano-GPT model, running on Apple M-series GPU via Metal. No extra setup needed.

### Python Sidecar — Bring Your Own Model

```bash
# Terminal 1: start the Python trainer (any HuggingFace causal LM)
pip install -r scripts/requirements.txt
python scripts/vram_trainer.py --model gpt2 --device cuda

# Terminal 2: start the Rust miner pointing at the sidecar
VRAMHUB_SIDECAR_URL=http://127.0.0.1:17070 \
  cargo run --release --bin vramhub-miner --features sidecar
```

The sidecar exposes a local HTTP API at `VRAMHUB_SIDECAR_URL` (default `http://127.0.0.1:17070`; Windows reserves 7009-7108).
The Rust miner handles all chain/R2/compression logic; Python handles training.
Supports any HuggingFace causal LM: `gpt2`, `mistral`, `llama-3`, etc.

Sidecar endpoints:

| Endpoint | Description |
|----------|-------------|
| `POST /train` | Run one training step, return gradient + loss |
| `POST /load_checkpoint` | Load model weights from base64 bytes |
| `POST /save_checkpoint` | Serialize model weights to base64 |
| `POST /forward_loss` | Compute loss without a training step |
| `GET /health` | Returns model name and device |

### Templar Adapter

```bash
cargo run --release --bin vramhub-miner --features templar
```

Integrates with the [Templar](https://github.com/tplr-ai/templar) distributed training framework.

### Custom Adapter

Implement `TrainingFrameworkAdapter` in `crates/vramhub-adapter/src/`:

```rust
#[async_trait]
pub trait TrainingFrameworkAdapter: Send + Sync {
    fn name(&self) -> &str;

    async fn load_checkpoint(&mut self, bytes: &[u8]) -> Result<(), SlclError>;

    async fn train_step(&mut self, batch: &[i64]) -> Result<Vec<f32>, SlclError>;

    fn compress_gradient(&self, raw: &[f32]) -> Result<CompressedGradient, SlclError>;

    fn decompress_gradient(&self, compressed: &[u8]) -> Result<Vec<f32>, SlclError>;

    async fn apply_gradient(&mut self, gradient: &[f32], beta: f32) -> Result<(), SlclError>;

    async fn save_checkpoint(&self) -> Result<Checkpoint, SlclError>;

    fn get_assigned_batch(&self, uid: PeerId, window: WindowId) -> Vec<i64>;

    async fn forward_loss(&self, batch: &[i64]) -> Result<f32, SlclError>;
}
```

## Gradient Compression

All adapters share the same compression pipeline in `crates/vramhub-adapter/src/training.rs`:

- **Top-K selection** — keeps the `topk_compression`% largest-magnitude elements
- **f16 quantization** — values stored as IEEE 754 half-precision (2 bytes vs 4)
- **Wire format**: `[total_params u32][k u32][k × index u32][k × f16 value]`
- **Typical ratio**: ~50–100× at top-1% (e.g. 4 MB → 40 KB for a 1M param model)

## Log Output

Healthy miner output looks like:

```
INFO vramhub_miner: Miner starting adapter=nano-gpt-10m device=cuda:0
INFO vramhub_miner::miner: Starting window window=2957300 uid=42
INFO vramhub_miner::miner: Train step window=2957300 uid=42 loss=3.2141 grad_params=10285056
INFO vramhub_miner::miner: Gradient compressed window=2957300 uid=42 size_bytes=82280 hash=a3f...
INFO vramhub_miner::miner: Gradient uploaded window=2957300 uid=42 r2_key=gradient/2957300/42.bin
```

If you see `Window failed` errors, check:
- R2 credentials are correct (`VRAMHUB_R2_*` vars)
- Your wallet has enough SUI for transaction fees
- Sidecar is running (if using `--features sidecar`)

## Monitoring

Check your current score and rewards:

```bash
cargo run --release --bin vramhub-cli -- scores --uid $VRAMHUB_MINER_UID
```

Check recent window activity:

```bash
cargo run --release --bin vramhub-cli -- status
```

Watch live on VRAMScan: http://localhost:4322/miners

## Systemd Service (Production)

```ini
[Unit]
Description=VRAM HUB Miner
After=network.target

[Service]
Type=simple
User=vram
WorkingDirectory=/opt/vram-hub
EnvironmentFile=/opt/vram-hub/.env
ExecStart=/opt/vram-hub/target/release/vramhub-miner
Restart=on-failure
RestartSec=10

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl enable --now vram-miner
sudo journalctl -u vram-miner -f
```
