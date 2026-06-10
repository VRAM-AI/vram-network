# Python Sidecar — Mining with Any PyTorch Model

The Python sidecar lets you participate as a VRAM miner using any HuggingFace causal language model — without modifying or recompiling the Rust miner. It is the recommended way to mine on testnet today.

---

## Why the Sidecar Exists

The Rust miner handles all protocol concerns: Sui registration, Cloudflare R2 uploads, gradient compression, checkpoint anchoring, and reward collection. What it cannot do natively (yet) is run a large PyTorch training loop — the pure-Rust [Candle adapter](../../crates/vramhub-adapter/src/candle_gpt.rs) is limited to a small nano-GPT and is still maturing ([bounty #15](https://github.com/VRAM-AI/VRAM-HUB/issues/15)).

The sidecar bridges this gap: a small Flask HTTP server wraps your training script and exposes a fixed protocol. The Rust miner calls it for every training step. Rust owns the network; Python owns the GPU.

```
┌───────────────────────────────┐     HTTP on 127.0.0.1:17070     ┌──────────────────────────────┐
│      vramhub-miner  (Rust)    │                                  │   vram_trainer.py  (Python)  │
│                               │  POST /train ─────────────────▶  │                              │
│  • Sui registration           │  ◀───── { gradient, loss } ─────  │  • loads HuggingFace model   │
│  • window clock               │  POST /save_checkpoint ────────▶  │  • forward + backward pass   │
│  • top-K f16 compression      │  POST /load_checkpoint ────────▶  │  • AdamW optimizer step      │
│  • R2 upload                  │  POST /forward_loss ───────────▶  │  • returns raw gradients     │
│  • checkpoint anchoring       │  GET  /health ─────────────────▶  │  • any HuggingFace model     │
│  • token reward collection    │                                  │                              │
└───────────────────────────────┘                                  └──────────────────────────────┘
```

---

## Quick Start

### CPU (any machine, no GPU required)

```bash
# Terminal 1 — start the sidecar with gpt2-124M on CPU
pip install -r scripts/requirements.txt
python scripts/vram_trainer.py --model gpt2 --device cpu

# Terminal 2 — start the miner
RUST_LOG=info VRAMHUB_SIDECAR_URL=http://127.0.0.1:17070 \
  cargo run --release --bin vramhub-miner --features sidecar
```

### NVIDIA GPU

```bash
# Terminal 1
pip install -r scripts/requirements.txt
python scripts/vram_trainer.py --model gpt2 --device cuda

# Terminal 2
RUST_LOG=info cargo run --release --bin vramhub-miner --features sidecar
```

### Apple Silicon (MPS)

```bash
# Terminal 1
python scripts/vram_trainer.py --model gpt2 --device mps

# Terminal 2
RUST_LOG=info cargo run --release --bin vramhub-miner --features sidecar
```

The miner auto-registers on first startup and saves your UID to `.vramhub-uid`. You will see it printed on startup — use it to track your scores on VRAMScan.

---

## Sidecar CLI Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--port` | `17070` | HTTP port to listen on |
| `--model` | `gpt2` | HuggingFace model name or local path |
| `--device` | `auto` | `auto` \| `cpu` \| `cuda` \| `cuda:N` \| `mps` |
| `--lr` | `3e-4` | AdamW learning rate |
| `--batch` | `4` | Sequences per training step |
| `--seqlen` | `256` | Token sequence length |

`auto` device selection: CUDA → MPS → CPU, whichever is available.

---

## Using a Larger Model

Any `AutoModelForCausalLM`-compatible model works. Pass the HuggingFace repo ID:

```bash
# 1.3B — needs ~6 GB VRAM
python scripts/vram_trainer.py --model EleutherAI/pythia-1.4b --device cuda

# 7B — needs ~14 GB VRAM (bf16 or quantized)
python scripts/vram_trainer.py --model mistralai/Mistral-7B-v0.1 --device cuda
```

Larger models produce more informative gradients and tend to score higher with validators. The tradeoff is longer training steps — each window is 10 minutes, so your step needs to complete well within that window.

---

## HTTP Protocol

The Rust miner calls these endpoints. You only need to understand this if you are writing your own sidecar.

### `GET /health`

Returns sidecar status. The miner polls this at startup and retries for up to 60 seconds.

```
Response: { "status": "ok", "model": "gpt2@124M", "device": "cuda:0" }
```

### `POST /train`

Runs a full forward + backward pass and returns the compressed gradient and loss.

```
Request:  { "uid": 42, "window": 1001 }
Response: { "gradient": [0.0, ..., 0.003, ...], "loss": 3.471 }
```

`uid` and `window` are used as a deterministic seed to select the training batch. The sidecar uses them to load a consistent slice of FineWeb-edu (or falls back to synthetic LCG tokens if the dataset is unavailable). This ensures validators can reproduce the same loss measurement.

The gradient is returned as a flat list of `f32` values. The sidecar pre-applies top-K sparsification (top 0.1% by magnitude) before returning — this keeps the JSON payload under ~1.5 MB even for 7B models.

### `POST /forward_loss`

Runs a forward-only pass (no gradient). Used by validators to measure loss delta without training.

```
Request:  { "uid": 42, "window": 1001 }
Response: { "loss": 3.471 }
```

### `POST /save_checkpoint`

Serializes the current model state to `torch.save` format, base64-encoded.

```
Request:  {}
Response: { "data": "<base64 bytes>" }
```

### `POST /load_checkpoint`

Loads a previously saved checkpoint into the model. Called by the miner after downloading the latest aggregated checkpoint from R2.

```
Request:  { "data": "<base64 bytes>" }
Response: { "ok": true }
```

---

## Data Loading

The sidecar tries to load training data from HuggingFace's FineWeb-edu dataset (`sample-10BT`, streaming) using `(uid, window)` as a deterministic offset. If the dataset is unavailable (no internet, rate-limited, etc.), it falls back to synthetic tokens generated by an LCG seeded from `uid * 1_000_003 + window`.

Both paths produce consistent batches for the same `(uid, window)` pair, which matters because validators re-run `forward_loss` to check your submitted loss is real.

If you want to train on your own dataset, replace the `_get_batch` function in `scripts/vram_trainer.py`. The only constraint: given the same `(uid, window)` inputs, return the same batch every time.

---

## Writing Your Own Sidecar

You can replace `vram_trainer.py` with any HTTP server that implements the five endpoints above. The constraints:

1. **Bind to `127.0.0.1`**, not `0.0.0.0`. The miner connects over loopback by default. Exposing to the network leaks raw gradients and checkpoint weights.
2. **Deterministic batches.** For a given `(uid, window)`, `POST /train` and `POST /forward_loss` must use the same data. Validators check your loss independently.
3. **Serialize training steps.** The miner calls `/train` synchronously — only one step runs at a time per process. You do not need to handle concurrent training calls.
4. **Return raw f32 gradients.** The Rust miner applies its own top-K f16 compression on top before uploading to R2. Do not double-compress.

A minimal sidecar skeleton in any language only needs ~100 lines. See `crates/vramhub-adapter/src/sidecar.rs` for the Rust client side and its full endpoint documentation.

---

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `VRAMHUB_SIDECAR_URL` | `http://127.0.0.1:17070` | URL the miner uses to reach the sidecar |

Set this if you change `--port` or run the sidecar on a different host (e.g. a GPU server). If running on separate machines, add authentication — the sidecar has none by default.

---

## Gradient Compression

The sidecar pre-sparsifies the gradient before returning it to the Rust miner:

1. Flatten all parameter gradients into a single vector
2. Select top 0.1% entries by absolute magnitude (`TOPK_FRAC = 0.001`)
3. Zero out the rest
4. Return the full-length sparse vector as JSON

The Rust miner then applies a second compression pass (top-K f16, DCT domain) before uploading to R2. This two-stage approach keeps network payloads small even for 7B+ parameter models.

For reference: a 124M GPT-2 has ~124M parameters. At 0.1%, ~124k values are non-zero. Each `f32` is 4 bytes → ~500 KB payload before the Rust compression pass.

---

## Troubleshooting

**Miner exits with "Sidecar at http://127.0.0.1:17070 did not respond within 60s"**

The sidecar is not running or is on a different port. Start the Python process first, wait for it to print `VRAM sidecar listening`, then start the miner.

**On Windows: "OSError: [WinError 10013] An attempt was made to access a socket in a way forbidden by its access permissions"**

Windows dynamically reserves the port range 7009-7108 (and other ranges) for Hyper-V, which blocks ports in the 7xxx range including the original default of 7070. The current default is 17070, which is well outside all Windows exclusions. If you picked a custom port in the 7xxx range, change it: `python scripts/vram_trainer.py --port 17070`. You can see all excluded ranges with `netsh interface ipv4 show excludedportrange protocol=tcp`.

**Sidecar crashes on large model with OOM**

Reduce `--batch` or `--seqlen`, or switch to a smaller model. The gradient size scales with model parameters, not batch size. A 7B model at `--batch 1 --seqlen 128` uses significantly less memory than the defaults.

**Loss is always the same / not changing**

Check the sidecar logs — it should print `train uid=N window=N loss=X.XX` each step. If `loss` is constant, the optimizer may not be stepping (optimizer.zero_grad / backward / step cycle). This is a bug in a custom sidecar, not something that can happen with the default `vram_trainer.py`.

**ModuleNotFoundError: No module named 'datasets'**

The FineWeb-edu loader requires the `datasets` package. Install it:

```bash
pip install datasets
```

Or just ignore it — the sidecar falls back to synthetic tokens automatically.

---

## See Also

- [Adding a Training Adapter](../../CONTRIBUTING.md#adding-a-training-adapter) — write a native Rust adapter instead
- [Environment Variables](../reference/env-vars.md) — full list of `VRAMHUB_*` vars
- [Miner Setup](setup.md) — wallet, R2, and testnet registration
