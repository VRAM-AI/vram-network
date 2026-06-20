#!/usr/bin/env python3
"""
VRAM Network — Python Sidecar Trainer
======================================
Implements the sidecar HTTP protocol so any PyTorch training script can
act as a VRAM miner. The Rust miner daemon handles chain / Walrus / compression;
this script handles the actual forward/backward pass.

Usage:
  pip install -r python/requirements.txt
  python python/vram_trainer.py [--port 17070] [--model gpt2] [--device cuda]

  # Gemma 4 E2B fine-tune on Move/DeepBook instruction dataset (RTX 4090):
  python python/vram_trainer.py \
    --model google/gemma-4-E2B-it \
    --dataset path/to/train_clean.jsonl \
    --device cuda \
    --dtype bfloat16 \
    --batch 2 \
    --seqlen 2048

  # Dataset can also be a Walrus blob URL or a GitHub raw URL:
  python python/vram_trainer.py \
    --model google/gemma-4-E2B-it \
    --dataset https://aggregator.walrus-testnet.walrus.space/v1/blobs/<blob_id> \
    --device cuda --dtype bfloat16

  # Note: Gemma requires HuggingFace login first:
  #   huggingface-cli login
  #   (accept license at https://huggingface.co/google/gemma-4-E2B-it)

  # Default port is 17070, NOT 7070. Windows dynamically reserves 7009-7108
  # for Hyper-V. 17070 is safe on all platforms.

Then in a second terminal:
  VRAMHUB_SIDECAR_URL=http://127.0.0.1:17070 vram-miner

Endpoints:
  POST /train              { uid, window }  → { gradient: [f32], loss: f32 }
  POST /load_checkpoint    { data: b64 }   → { ok: true }
  POST /save_checkpoint    {}              → { data: b64 }
  POST /forward_loss       { uid, window } → { loss: f32 }
  GET  /health                             → { status, model, device, dataset }
"""

import argparse
import base64
import io
import json
import logging
import pathlib
import random
import threading
import urllib.request
from typing import Optional, List, Dict

import torch
from flask import Flask, jsonify, request
from transformers import AutoModelForCausalLM, AutoTokenizer

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")
log = logging.getLogger("vram_trainer")

# ── CLI args ──────────────────────────────────────────────────────────────────

parser = argparse.ArgumentParser()
parser.add_argument("--port",           type=int,   default=17070,     help="HTTP port")
parser.add_argument("--model",          type=str,   default="gpt2",    help="HuggingFace model name or path")
parser.add_argument("--dataset",        type=str,   default=None,      help="JSONL instruction dataset: local path, URL (Walrus/GitHub raw), or 'fineweb'")
parser.add_argument("--device",         type=str,   default="auto",    help="Device: auto|cuda|cpu|mps")
parser.add_argument("--dtype",          type=str,   default="auto",    help="Weight dtype: auto|bfloat16|float16|float32")
parser.add_argument("--lr",             type=float, default=3e-4,      help="Learning rate")
parser.add_argument("--batch",          type=int,   default=4,         help="Batch size")
parser.add_argument("--seqlen",         type=int,   default=256,       help="Max sequence length in tokens")
parser.add_argument("--use-8bit-adam",  action="store_true",           help="Force bitsandbytes 8-bit AdamW")
args = parser.parse_args()

# ── Device + dtype ────────────────────────────────────────────────────────────

def pick_device(spec: str) -> torch.device:
    if spec == "auto":
        if torch.cuda.is_available():             return torch.device("cuda")
        if hasattr(torch.backends, "mps") and torch.backends.mps.is_available():
            return torch.device("mps")
        return torch.device("cpu")
    return torch.device(spec)

def pick_dtype(spec: str, device: torch.device) -> torch.dtype:
    if spec == "auto":
        return torch.bfloat16 if device.type == "cuda" else torch.float32
    return {"bfloat16": torch.bfloat16, "float16": torch.float16, "float32": torch.float32}.get(spec, torch.float32)

DEVICE = pick_device(args.device)
DTYPE  = pick_dtype(args.dtype, DEVICE)
log.info(f"Device: {DEVICE}  dtype: {DTYPE}")

# ── Model ─────────────────────────────────────────────────────────────────────

log.info(f"Loading {args.model!r} …")
_load_kwargs: dict = {
    "device_map": {"": DEVICE} if DEVICE.type != "cpu" else "cpu",
    "low_cpu_mem_usage": True,
}
# transformers ≥5.x renamed torch_dtype → dtype; fall back for older builds
import inspect as _inspect
_fp_sig = _inspect.signature(AutoModelForCausalLM.from_pretrained)
if "dtype" in _fp_sig.parameters:
    _load_kwargs["dtype"] = DTYPE
else:
    _load_kwargs["torch_dtype"] = DTYPE
model = AutoModelForCausalLM.from_pretrained(args.model, **_load_kwargs)
model.train()

if hasattr(model, "gradient_checkpointing_enable"):
    model.gradient_checkpointing_enable()
    log.info("Gradient checkpointing enabled")

tokenizer = AutoTokenizer.from_pretrained(args.model)
if tokenizer.pad_token is None:
    tokenizer.pad_token = tokenizer.eos_token

def _param_count() -> int:
    return sum(p.numel() for p in model.parameters())

def _make_optimizer():
    if args.use_8bit_adam or _param_count() > 500_000_000:
        try:
            import bitsandbytes as bnb
            log.info("Using bitsandbytes 8-bit AdamW")
            return bnb.optim.AdamW8bit(model.parameters(), lr=args.lr)
        except ImportError:
            log.warning("bitsandbytes not installed — falling back to fp32 AdamW (may OOM on large models)")
    return torch.optim.AdamW(model.parameters(), lr=args.lr)

optimizer  = _make_optimizer()
model_lock = threading.Lock()
MODEL_NAME = f"{args.model}@{_param_count()//1_000_000}M"
log.info(f"Model ready: {MODEL_NAME}")

if DEVICE.type == "cuda":
    free, total = torch.cuda.mem_get_info()
    log.info(f"VRAM after load: {(total-free)/1e9:.1f}/{total/1e9:.1f} GB")

# ── Dataset ───────────────────────────────────────────────────────────────────

# Detect whether the model uses a chat template (instruction-tuned models like
# Gemma-it, Llama-instruct, Qwen-chat). If so, we wrap pairs in the template.
HAS_CHAT_TEMPLATE = (
    hasattr(tokenizer, "chat_template") and tokenizer.chat_template is not None
)
log.info(f"Chat template: {'yes' if HAS_CHAT_TEMPLATE else 'no (pretraining format)'}")

# ── JSONL instruction dataset ────────────────────────────────────────────────

INSTRUCTION_ROWS: Optional[List[Dict]] = None

def _load_jsonl_from_url(url: str) -> List[Dict]:
    log.info(f"Fetching dataset from {url} …")
    with urllib.request.urlopen(url, timeout=60) as resp:
        raw = resp.read().decode("utf-8")
    rows = [json.loads(l) for l in raw.splitlines() if l.strip()]
    log.info(f"Loaded {len(rows)} rows from URL")
    return rows

def _load_jsonl_from_path(path: str) -> List[Dict]:
    p = pathlib.Path(path)
    rows = [json.loads(l) for l in p.read_text(encoding="utf-8").splitlines() if l.strip()]
    log.info(f"Loaded {len(rows)} rows from {path}")
    return rows

def _init_dataset():
    global INSTRUCTION_ROWS
    spec = args.dataset
    if spec is None or spec == "fineweb":
        log.info("Dataset: FineWeb-edu (pretraining mode)")
        return
    if spec.startswith("http://") or spec.startswith("https://"):
        INSTRUCTION_ROWS = _load_jsonl_from_url(spec)
    else:
        INSTRUCTION_ROWS = _load_jsonl_from_path(spec)
    if not INSTRUCTION_ROWS:
        raise ValueError(f"Dataset loaded 0 rows from {spec!r}")
    log.info(f"Dataset: instruction JSONL — {len(INSTRUCTION_ROWS)} rows, chat_template={HAS_CHAT_TEMPLATE}")

_init_dataset()

def _format_instruction_row(row: Dict) -> str:
    """
    Convert an {instruction, input, output} row to a training string.
    Uses the tokenizer's chat template for instruction-tuned models (Gemma-it,
    Llama-instruct, etc.); falls back to a plain prompt format otherwise.
    """
    instruction = row.get("instruction", "")
    inp         = row.get("input", "")
    output      = row.get("output", "")

    user_content = instruction
    if inp:
        user_content = f"{instruction}\n\n{inp}"

    if HAS_CHAT_TEMPLATE:
        messages = [
            {"role": "user",      "content": user_content},
            {"role": "assistant", "content": output},
        ]
        # add_generation_prompt=False so the answer is included in the loss
        return tokenizer.apply_chat_template(
            messages,
            tokenize=False,
            add_generation_prompt=False,
        )
    else:
        return f"### Instruction\n{user_content}\n\n### Response\n{output}"

def _sample_instruction_batch(uid: int, window: int) -> torch.Tensor:
    """Sample a deterministic-ish batch from the instruction rows."""
    rng = random.Random(uid * 1_000_003 + window)
    selected = rng.choices(INSTRUCTION_ROWS, k=args.batch)
    texts = [_format_instruction_row(r) for r in selected]
    enc = tokenizer(
        texts,
        truncation=True,
        max_length=args.seqlen + 1,
        padding="max_length",
        return_tensors="pt",
    )
    return enc["input_ids"].to(DEVICE)   # (B, seqlen+1)

# ── FineWeb-edu fallback ──────────────────────────────────────────────────────

def _lcg_tokens(seed: int, n: int) -> torch.Tensor:
    vocab = model.config.vocab_size
    s     = seed & 0xFFFF_FFFF_FFFF_FFFF
    tokens = []
    for _ in range(n):
        s = (s * 6364136223846793005 + 1442695040888963407) & 0xFFFF_FFFF_FFFF_FFFF
        tokens.append(s % vocab)
    return torch.tensor(tokens, dtype=torch.long)

def _get_fineweb_batch(uid: int, window: int) -> torch.Tensor:
    seed  = (uid * 1_000_003 + window) & 0xFFFF_FFFF_FFFF_FFFF
    try:
        from datasets import load_dataset
        offset = int(seed % 100_000)
        ds = load_dataset("HuggingFaceFW/fineweb-edu", name="sample-10BT", split="train", streaming=True)
        texts = []
        for ex in ds.skip(offset):
            texts.append(ex["text"])
            if len(texts) >= args.batch:
                break
        enc = tokenizer(texts, truncation=True, max_length=args.seqlen+1, padding="max_length", return_tensors="pt")
        return enc["input_ids"].to(DEVICE)
    except Exception:
        log.debug("FineWeb unavailable — using synthetic tokens")
        rows = []
        for b in range(args.batch):
            rows.append(_lcg_tokens(seed + b, args.seqlen + 1))
        return torch.stack(rows).to(DEVICE)

# ── Unified batch getter ──────────────────────────────────────────────────────

def _get_batch(uid: int, window: int) -> torch.Tensor:
    if INSTRUCTION_ROWS is not None:
        return _sample_instruction_batch(uid, window)
    return _get_fineweb_batch(uid, window)

def _compute_loss(uid: int, window: int):
    batch   = _get_batch(uid, window)     # (B, T+1)
    inputs  = batch[:, :-1]               # (B, T)
    targets = batch[:, 1:]                # (B, T)
    out     = model(input_ids=inputs, labels=targets)
    return out.loss, out.loss.item()

# ── HTTP server ───────────────────────────────────────────────────────────────

app = Flask(__name__)

TOPK_FRAC = 0.001  # top 0.1% of gradients — ~1.5 MB payload for 2B model

@app.route("/health")
def health():
    info = {
        "status":   "ok",
        "model":    MODEL_NAME,
        "device":   str(DEVICE),
        "dtype":    str(DTYPE),
        "dataset":  args.dataset or "fineweb",
        "dataset_rows": len(INSTRUCTION_ROWS) if INSTRUCTION_ROWS else None,
        "chat_template": HAS_CHAT_TEMPLATE,
    }
    if DEVICE.type == "cuda":
        free, total = torch.cuda.mem_get_info()
        info["vram_used_gb"]  = round((total - free) / 1e9, 2)
        info["vram_total_gb"] = round(total / 1e9, 2)
    return jsonify(info)

@app.route("/train", methods=["POST"])
def train():
    body   = request.get_json(force=True)
    uid    = int(body.get("uid",    0))
    window = int(body.get("window", 0))

    with model_lock:
        optimizer.zero_grad()
        loss, loss_val = _compute_loss(uid, window)
        loss.backward()

        # Move gradients to CPU immediately to avoid holding a 2B-param tensor in VRAM
        grad_parts = []
        for p in model.parameters():
            if p.grad is not None:
                grad_parts.append(p.grad.detach().float().cpu().flatten())
            else:
                grad_parts.append(torch.zeros(p.numel(), dtype=torch.float32))
        all_grads = torch.cat(grad_parts)   # (N,) on CPU

        topk = max(1, int(all_grads.numel() * TOPK_FRAC))
        _, indices = torch.topk(all_grads.abs(), topk)
        sparse = torch.zeros_like(all_grads)
        sparse[indices] = all_grads[indices]
        grads = sparse.tolist()

        optimizer.step()

    log.info(f"train uid={uid} window={window} loss={loss_val:.4f} params={all_grads.numel():,} topk={topk:,}")
    return jsonify({"gradient": grads, "loss": loss_val})

@app.route("/forward_loss", methods=["POST"])
def forward_loss():
    body   = request.get_json(force=True)
    uid    = int(body.get("uid",    0))
    window = int(body.get("window", 0))
    with model_lock:
        with torch.no_grad():
            _, loss_val = _compute_loss(uid, window)
    return jsonify({"loss": loss_val})

@app.route("/load_checkpoint", methods=["POST"])
def load_checkpoint():
    body = request.get_json(force=True)
    data = base64.b64decode(body["data"])
    buf  = io.BytesIO(data)
    with model_lock:
        state = torch.load(buf, map_location=DEVICE, weights_only=True)
        model.load_state_dict(state, strict=False)
    log.info(f"Loaded checkpoint ({len(data):,} bytes)")
    return jsonify({"ok": True})

@app.route("/save_checkpoint", methods=["POST"])
def save_checkpoint():
    buf = io.BytesIO()
    with model_lock:
        torch.save(model.state_dict(), buf)
    data = base64.b64encode(buf.getvalue()).decode()
    log.info(f"Saved checkpoint ({buf.tell():,} bytes)")
    return jsonify({"data": data})

if __name__ == "__main__":
    log.info(f"VRAM sidecar listening on 0.0.0.0:{args.port}")
    app.run(host="0.0.0.0", port=args.port, threaded=False)
