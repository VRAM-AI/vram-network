# VRAM HUB — Architecture

VRAM HUB is a decentralized LLM training coordination protocol. It replaces every trust assumption in distributed training — the coordinator, the evaluator, the scorer — with cryptographic guarantees: on-chain coordination via Sui, gradient privacy via Seal IBE, and loss verification via AWS Nitro Enclaves.

This document explains how all of it works, end to end.

---

## The Three Pillars

Most distributed training systems have a hidden trust assumption: *someone* decides whose gradient was good. In Bittensor that's validator consensus. In Templar that's a trusted coordinator. VRAM HUB eliminates the assumption entirely.

The three mechanisms that make this possible:

| Mechanism | What It Guarantees |
|-----------|-------------------|
| **AWS Nitro TEE** | Loss evaluation is unforgeable. The scoring binary is committed to on-chain PCR values; a modified scorer cannot register. |
| **Sui Seal IBE** | Only staked validators can read miner R2 credentials. No central server holds the decryption key. |
| **Sui blockchain** | Every registration, score, and reward is anchored on-chain. No coordinator needed for window progression or reward distribution. |

Compromising the system requires breaking all three simultaneously.

---

## System Overview

```
                          ┌──────────────────────────────────────────────────────┐
                          │                    Sui Blockchain                    │
                          │                                                      │
                          │  peer_registry      validator_registry               │
                          │  enclave_registry   score_ledger                     │
                          │  round_state        hparams                          │
                          │  reward_distributor seal_policy   vram_token         │
                          └────┬───────────────────────────────┬─────────────────┘
                               │                               │
               register        │  window / hparams             │  submit_scores
               read enclave    │  read checkpoint hash         │  read enclave pubkey
               pubkey          │                               │
                               ▼                               ▼
┌──────────────────────────────┐              ┌──────────────────────────────────┐
│           Miner              │              │            Validator              │
│                              │              │                                  │
│  ┌────────────────────────┐  │              │  1. Seal IBE — decrypt R2 creds  │
│  │  vramhub-miner (Rust)  │  │              │  2. Download gradient from R2    │
│  │                        │  │              │  3. Send to Nautilus enclave      │
│  │  • window clock        │  │  gradient    │  4. Get Ed25519-signed score      │
│  │  • R2 upload      ─────┼──┼────────────►│  5. submit_scores on-chain        │
│  │  • top-K f16 compress  │  │              └────────────────┬─────────────────┘
│  │  • checkpoint anchor   │  │                               │
│  └────────┬───────────────┘  │                               ▼
│           │ HTTP :17070      │              ┌──────────────────────────────────┐
│  ┌────────▼───────────────┐  │              │     Nautilus Enclave (Nitro TEE) │
│  │  vram_trainer.py       │  │              │                                  │
│  │  (Python sidecar)      │  │              │  • hardware-isolated VM          │
│  │                        │  │              │  • computes loss before/after    │
│  │  • HuggingFace model   │  │              │  • OpenSkill rating update       │
│  │  • forward/backward    │  │              │  • Ed25519 sign via NSM          │
│  │  • AdamW optimizer     │  │              │  • private key never leaves TEE  │
│  └────────────────────────┘  │              └──────────────────────────────────┘
└──────────────────────────────┘
```

---

## Windows — Coordination Without a Coordinator

The network advances in synchronized **windows** (default: 10 minutes each). Every node derives the current window deterministically from the on-chain `RoundState` object:

```
window = floor((current_epoch_ms - genesis_ms) / window_duration_ms)
```

No heartbeat, no coordinator. Every miner and validator independently knows which window is active, when it opens, and when it closes — purely from on-chain state.

**Within each window:**

```
t=0   Window opens
       ├── Miners load latest checkpoint from R2
       ├── Miners train on their assigned batch (uid × window seed)
       └── Miners upload compressed gradient + anchor hash on-chain

t≈5m  Validators begin evaluation
       ├── Validators decrypt miner R2 credentials via Seal IBE
       ├── Validators download gradients from R2
       ├── Validators send gradients to Nautilus enclave
       └── Enclave returns signed loss-delta scores

t≈9m  Score submission window
       ├── Validators submit signed scores to score_ledger.move
       └── reward_distributor.move emits VRAM tokens proportional to OpenSkill weights

t=10m Next window opens
```

The miner's upload deadline (`put_window_open_ms` into each window) and the validator's submission deadline are both enforced on-chain. Late submissions are rejected by `score_ledger.move`.

---

## Miner Architecture — Rust Owns the Network, Python Owns the GPU

The miner is split into two processes that communicate over localhost HTTP. This is the **sidecar architecture**.

```
┌─────────────────────────────────┐    HTTP 127.0.0.1:17070    ┌─────────────────────────────────┐
│      vramhub-miner  (Rust)      │                            │   vram_trainer.py  (Python)     │
│                                 │  POST /train ────────────► │                                 │
│  Protocol:                      │  ◄──── { gradient, loss }  │  Training:                      │
│  • Sui registration             │                            │  • loads HuggingFace model      │
│  • window clock from chain      │  POST /save_checkpoint ──► │  • forward pass on assigned     │
│  • top-K f16 DCT compression    │  POST /load_checkpoint ──► │    data batch                   │
│  • R2 gradient upload           │  POST /forward_loss ─────► │  • backward pass (AdamW)        │
│  • checkpoint hash on-chain     │  GET  /health ───────────► │  • returns sparse gradient      │
│  • token reward collection      │                            │  • handles checkpoint I/O       │
└─────────────────────────────────┘                            └─────────────────────────────────┘
```

### Why Two Processes?

The Rust miner is lean and correct: it handles everything that touches the chain, R2, or compression. The Python sidecar is flexible: it handles everything that touches GPU memory and model weights. This separation means:

- **Any model** works without modifying or recompiling the miner — pass a different `--model` flag to the sidecar
- **Any framework** works — the HTTP protocol is simple enough to implement in JAX, TF, or any custom training loop
- **The protocol layer never goes OOM** — gradient compression happens in Rust with deterministic memory bounds

The Rust miner also supports a native Candle adapter for the nano-GPT (6-layer, 384-dim, ~10M params) via `--features candle|cuda|metal`, but the sidecar is recommended for testnet because it supports arbitrary HuggingFace models.

### Sidecar Protocol

Five endpoints over HTTP on `127.0.0.1:17070` (default; Windows reserves 7009-7108 so 7070 is unusable there):

| Endpoint | Direction | Purpose |
|----------|-----------|---------|
| `GET /health` | Rust → Python | Startup check; Rust waits up to 60s |
| `POST /train` | Rust → Python | Run full train step; returns `{ gradient, loss }` |
| `POST /forward_loss` | Rust → Python | Loss-only (no update); used by validator for verification |
| `POST /save_checkpoint` | Rust → Python | Serialize model state to base64 for R2 upload |
| `POST /load_checkpoint` | Rust → Python | Restore model state from base64 |

`POST /train` and `POST /forward_loss` both take `{ uid, window }` and use them as a deterministic seed for data assignment (see Data Assignment below).

---

## Gradient Compression

Gradients from large models are enormous — a 7B parameter model has ~28 GB of raw f32 gradients. VRAM HUB compresses these in two stages:

### Stage 1 — Python Sidecar (top-K sparsification)

The sidecar pre-filters the gradient before returning it to Rust:

```python
TOPK_FRAC = 0.001   # keep top 0.1% by absolute magnitude

all_grads = torch.cat([p.grad.flatten() for p in model.parameters()])
topk = max(1, int(all_grads.numel() * TOPK_FRAC))
_, indices = torch.topk(all_grads.abs(), topk)
sparse = torch.zeros_like(all_grads)
sparse[indices] = all_grads[indices]
```

Result: a 7B model (28 GB raw) → ~28 MB sparse vector. A 124M GPT-2 → ~500 KB.

### Stage 2 — Rust Miner (top-K f16 DCT)

The Rust miner applies a second compression in the DCT domain:

```
1. Apply momentum accumulation:  m[t+1] = γ · m[t] + η · g[t]
2. DCT transform the momentum buffer
3. Keep top-K DCT coefficients by magnitude
4. Quantize surviving coefficients to f16
5. Serialize as (index, value) pairs
```

This two-stage approach is robust across model sizes. The DCT step concentrates energy into fewer coefficients (gradients tend to be low-frequency); the f16 quantization halves storage again.

The R2 key for each gradient is: `gradient-{window}-{uid}-v{version}.pt`

---

## Data Assignment — Deterministic and Verifiable

Each miner trains on a unique but reproducible data subset each window:

```
seed = SHA256(uid || window)
pages = Sample(FineWeb-edu, seed, batch_size)
```

The same formula is used by validators when they run `forward_loss`. This means validators can independently verify the loss the miner claimed — without trusting the miner's self-report. The data is not delivered to miners by a coordinator; miners derive their batch independently and validators verify independently.

The Python sidecar implements this in `_get_batch(uid, window)`:
- Primary: streaming FineWeb-edu (`sample-10BT`) offset by `seed % 100_000`
- Fallback: LCG-generated synthetic tokens (same seed, fully offline)

Both paths produce identical batches for the same `(uid, window)` — the fallback is indistinguishable to the scoring enclave.

---

## Validator Architecture — Seal IBE + Nautilus Enclave

The validator pipeline has three stages that run each window: credential decryption, gradient pre-filtering, and enclave evaluation.

### Stage 1 — Seal IBE Decryption

When a miner registers, their R2 read credentials are **Identity-Based Encrypted** with Sui Seal before being stored on-chain. Only validators that pass the `seal_approve` gate can decrypt them.

```
Miner registration:
  credentials = IBE_encrypt(r2_key + r2_secret, identity=validator_set)
  peer_registry.move ← stores the ciphertext

Validator decryption each window:
  1. Construct a PTB that calls seal_approve in seal_policy.move
  2. Send PTB to Seal key servers (threshold t-of-n)
  3. Key servers simulate the PTB locally:
       ─ Is caller registered in validator_registry.move?
       ─ Does caller have stake ≥ min_validator_stake?
       ─ Is caller marked active?
     All checks pass → key servers release their IBE key fragment
  4. Validator reconstructs the IBE decryption key from t fragments
  5. Decrypt R2 credentials → download gradient
```

No single key server can leak credentials. The master IBE key does not exist as a single value anywhere — it is reconstructed from fragments on demand, only by validators that pass the stake check.

### Stage 2 — Fast Eval Pre-Filter

Before sending anything to the enclave, the validator runs a cheap local check (`fast_eval.rs`) on every gradient:

```
For each miner gradient:
  ✓ Liveness: gradient_bytes is not empty
  ✓ Size:     gradient_bytes.len() ≤ MAX_GRADIENT_SIZE_BYTES
  ✓ Sync:     cosine similarity to aggregation (TODO — currently passes all)

  If any check fails → skip this miner entirely (not sent to enclave)
  phi = 1.0 (passes) or 0.0 (excluded)
```

This gate exists for two reasons: (1) avoid sending garbage to the enclave, which is expensive, and (2) protect the enclave from OOM on oversized payloads. The sync score check (cosine similarity to the aggregated gradient direction) is **not yet implemented** — currently all well-formed gradients pass this check automatically. It will be added before mainnet to detect miners training on wrong data or in the wrong direction.

### Stage 3 — Nautilus Enclave Evaluation

Gradients that pass fast eval are sent to the Nautilus enclave in a single batch call (`POST /process_data`):

**What the validator sends to the enclave:**

```json
{
  "window":             1042,
  "checkpoint_bytes":   "<full model weights>",
  "checkpoint_hash":    "<sha256 of checkpoint>",
  "peer_gradients":     { "uid_A": "<bytes>", "uid_B": "<bytes>", ... },
  "assigned_batches":   { "uid_A": [token_ids], "uid_B": [token_ids], ... },
  "random_batch":       [token_ids],
  "beta":               25.0,
  "current_ratings":    { "uid_A": [mu, sigma], "uid_B": [mu, sigma], ... }
}
```

**What happens inside the enclave:**

```
For each miner i:
  1. Verify checkpoint hash matches what's on-chain in round_state.move
  2. Load model weights from checkpoint_bytes
  3. loss_before = forward_pass(model, assigned_batch[i])
  4. Apply miner's gradient: model' = model + delta_i
  5. loss_after  = forward_pass(model', assigned_batch[i])
  6. score_i     = loss_before - loss_after   (positive = gradient helped)
  7. Update OpenSkill (mu, sigma) for miner i using current_ratings

Build signed payload:
  payload = { window, checkpoint_hash, scores: { uid → score } }
  signature = Ed25519_sign(payload, private_key_never_leaves_enclave)
```

**What the validator gets back:**

```json
{
  "scores":         { "uid_A": 142000, "uid_B": 87000, ... },
  "signed_payload": { "window": 1042, "checkpoint_hash": "...", "scores": {...} },
  "signature":      "<64 bytes Ed25519>"
}
```

The validator **verifies the signature locally** before submitting anything on-chain:

```rust
verify_response_signature(&enclave_pubkey, &payload_bytes, &signature)?;
// Only if this passes → chain.submit_scores(...)
```

This is not optional. If the enclave returns garbage or was tampered with mid-flight, the signature check catches it before an invalid score reaches the chain.

---

## Nautilus — The Three Modes

The validator has three distinct operating modes, controlled by environment variables. Understanding these is important for both operators and developers.

### Mode 1: Simulated (`VRAMHUB_TEST_MODE=true`)

No enclave. No signature. Scores come directly from `fast_eval` (gradient norm).

```
Who:    Any Linux VPS, no AWS account needed
Cost:   Free
Trust:  None — scores are self-reported
Chain:  Scores submitted with a zero signature (64 bytes of 0x00)
Result: VRAMScan shows SIMULATED badge
        Mainnet contracts REJECT this mode
```

Use this on testnet to develop validator logic, check the chain integration, and grow the scoring pool before mainnet. The VRAMScan `SIMULATED` badge makes it transparent to everyone.

### Mode 2: Dev (`VRAMHUB_NAUTILUS_URL` set, no other flags)

Real enclave. Signature verified locally. No cross-check against on-chain registry.

```
Who:    Developers with a local or dev enclave running
Cost:   EC2 dev instance
Trust:  Enclave is trusted by local pubkey fetch only
Chain:  Scores submitted with real enclave signature
Result: Suitable for testing the full pipeline locally
        Not production — anyone could swap the enclave binary
```

Use this when testing the Nitro pipeline locally or on a dev EC2 before running `register-enclave`.

### Mode 3: Nitro (`VRAMHUB_NITRO_ENCLAVE=true` + `VRAMHUB_ENCLAVE_PUBKEY=<hex>`)

Real enclave. Signature verified. Pubkey cross-checked against on-chain registry.

```
Who:    Production validator with registered enclave
Cost:   ~$36/month (c5.xlarge spot)
Trust:  Full — binary committed to on-chain PCRs, key pinned to registry
Chain:  Scores submitted with real enclave signature
Result: VRAMScan shows NITRO ENCLAVE badge
        The only mode accepted by mainnet contracts
```

At startup in Nitro mode, the validator fetches the enclave's public key from `GET /get_attestation` and compares it to `VRAMHUB_ENCLAVE_PUBKEY`. If they don't match, **the validator refuses to start**. This matters because the enclave generates a new keypair every time it boots (the key is ephemeral — it never persists to disk).

---

## Enclave Registration and PCR Binding

This is a one-time operation per enclave binary version. It is the expensive step that makes all per-window scoring cheap.

### What PCRs Are

PCRs (Platform Configuration Registers) are cryptographic hashes measured by the Nitro hardware at boot:

| Register | Covers | Size |
|----------|--------|------|
| PCR0 | OS image (kernel + ramdisk) | SHA-384, 48 bytes |
| PCR1 | Application binary (`vramhub-nautilus`) | SHA-384, 48 bytes |
| PCR2 | Application config / environment | SHA-384, 48 bytes |

These values are deterministic. The same binary always produces the same PCRs. A different binary — patched, malicious, or just recompiled with different options — produces different PCRs.

### The Registration Flow

```
Step 1: Enclave boots (on your EC2)
  ─ Generates ephemeral Ed25519 keypair (private key never leaves the TEE)
  ─ Requests a Nitro attestation document from the NSM (Nitro Security Module)
    The document contains: PCR0, PCR1, PCR2, enclave public key
    The document is signed by the AWS Nitro root CA (COSE_Sign1 format)
  ─ Serves the document at GET /get_attestation

Step 2: Operator runs register-enclave (one time)
  cargo run --bin vramhub-cli -- register-enclave \
    --enclave-url http://<EC2>:3000

  ─ CLI fetches the attestation document
  ─ Sends it to enclave_registry.move on Sui:
      register_enclave(attestation_doc, enclave_pubkey, pcr0, pcr1, pcr2)
  ─ On-chain verification:
      ✓ COSE_Sign1 signature against AWS Nitro root CA
      ✓ PCR0/PCR1/PCR2 match expected values in hparams.move
      ✓ Public key is valid 32-byte Ed25519
  ─ Full attestation doc stored on-chain for public auditability
  ─ Enclave public key recorded in EnclaveRegistry

Step 3: Copy the registered public key to .env
  VRAMHUB_ENCLAVE_PUBKEY=<hex printed by register-enclave>
```

### Per-Window Scoring (Cheap)

After registration, every window costs only one signature check:

```
score_ledger.move receives { window, checkpoint_hash, scores, signature }
  ✓ Look up enclave_pubkey from enclave_registry.move
  ✓ Ed25519_verify(signature, enclave_pubkey, payload) — single op
  ✓ checkpoint_hash matches round_state.move
  ✓ Record scores
```

The expensive attestation verification happened once. Everything after is a 64-byte signature check.

### The Restart Problem

The enclave key is **ephemeral** — it is generated fresh every time the enclave boots and never written to disk (the TEE has no persistent storage). This creates a specific operational hazard:

```
Normal operation:
  enclave running → pubkey = 0xABCD...
  VRAMHUB_ENCLAVE_PUBKEY = 0xABCD...  (registered on-chain)
  ✓ Validator starts, pubkey matches

After enclave restart:
  enclave rebooted → new keypair → pubkey = 0xEF01...
  VRAMHUB_ENCLAVE_PUBKEY = 0xABCD...  (still the old value)
  ✗ Validator refuses to start: "enclave pubkey mismatch"
```

> **WARNING — Production outage risk.** Any unplanned enclave reboot (EC2 maintenance, OOM kill, deployment) generates a new keypair and silently breaks all validators pointing at that enclave. Configure CloudWatch alarms on the enclave process and treat re-registration as a runbook step, not an afterthought.

When this happens you have two options:

**Option A — Re-register (recommended):**
```bash
cargo run --bin vramhub-cli -- register-enclave --enclave-url http://<EC2>:3000
# Update VRAMHUB_ENCLAVE_PUBKEY in .env with the new hex
# Restart the validator
```
This calls `update_enclave_key` on-chain (same PCR check, new pubkey).

**Option B — Keep the enclave alive:**
Configure the EC2 instance to auto-restart the enclave after crash, but accept that any reboot requires re-registration.

> **Why is the key ephemeral?** The Nitro TEE has no persistent writable storage. This is intentional — it prevents the private key from being extracted by anyone with long-term access to the EC2 instance. The tradeoff is that restarts require re-registration.

### Updating the Enclave Binary

If you ship a new version of `vramhub-nautilus` with a bug fix, the PCRs change:

1. Build the new EIF: `./scripts/build-enclave.sh` → note new PCR0/PCR1/PCR2
2. Governance call: `hparams.move::update_pcrs(new_pcr0, new_pcr1, new_pcr2)`
3. Re-register all enclaves: `vramhub-cli register-enclave ...`
4. Update `VRAMHUB_ENCLAVE_PUBKEY` in `.env` and restart validators

The PCR update is a governance operation — the deployer wallet must sign it. This prevents anyone from unilaterally substituting a different scoring binary.

### Current Status: COSE Verification

The on-chain `register_enclave` does full COSE_Sign1 verification against the AWS Nitro root CA. The Rust validator client (`attestation.rs`) currently does not re-verify the attestation document — it only cross-checks the pubkey against the on-chain registry. Full COSE verification in the Rust client is planned as a defence-in-depth measure.

In practice this does not weaken the security model: the on-chain verification is the authoritative check. The Rust-side pubkey cross-check guards against the enclave being silently replaced between registration and runtime.

---

## OpenSkill Rating — Bayesian Skill Estimation

Raw loss-delta scores are noisy. A single bad window should not destroy a miner's standing, and a single lucky window should not dominate. VRAM HUB uses the **Plackett-Luce OpenSkill model**.

### The Math

Each miner has a skill distribution $(\mu_i, \sigma_i)$. After each window:

**Loss delta score:**
$$s_i^t = L(\theta^t;\, D_i) - L(\theta^t + \delta_i;\, D_i)$$

Miners ranked by $s_i^t$. OpenSkill update:

$$c = \sqrt{\sum_i \sigma_i^2 + \beta^2 + n\beta^2}$$

$$\omega_i = \frac{1}{c}\left(1 - \sum_{q=0}^{r_i} \frac{e^{\mu_i/c}}{A_q}\right), \quad A_q = \sum_{j:\,\text{rank}(j)\,\geq\, q} e^{\mu_j/c}$$

$$\mu_i^{t+1} = \mu_i^t + \sigma_i^2 \cdot \omega_i$$
$$\sigma_i^{t+1} = \sigma_i^t \cdot \sqrt{1 - \sigma_i^2 \cdot \delta_i}$$

**Ordinal (conservative lower bound):**
$$\text{ord}_i = \mu_i - 3\sigma_i$$

New miners start with high $\sigma$ (uncertainty). As evidence accumulates, $\sigma$ shrinks and $\mu$ stabilizes. A consistently strong miner builds a high, tight distribution. A miner that degrades sees $\mu$ decay via the drift term $\tau$.

### Reward Weights

Rewards are proportional to squared ordinals:

$$w_i = \frac{(\text{ord}_i - \text{ord}_{\min})^2}{\sum_j (\text{ord}_j - \text{ord}_{\min})^2}$$

The squaring amplifies the gap between top and median performers. Mediocrity is not rewarded proportionally — sustained excellence compounds.

---

## Move Contracts

All coordination is anchored on Sui. The contracts are deployed at a fixed package ID with no upgrade authority on the core scoring logic.

| Contract | Responsibility |
|----------|---------------|
| `peer_registry.move` | Miner registration + IBE-encrypted R2 credentials. Maps peer UID → on-chain record. |
| `validator_registry.move` | Validator registration + stake tracking. Used by `seal_approve` to gate credential access. |
| `enclave_registry.move` | Stores registered Nitro enclave public keys and PCR values. |
| `score_ledger.move` | Ed25519 signature verification + per-window score storage. OpenSkill state lives here. |
| `round_state.move` | Current window number + anchored checkpoint hash. All nodes sync from this. |
| `hparams.move` | Governance-updatable parameters: window duration, compression ratio, PCR values, min stake, emission rate. |
| `reward_distributor.move` | Emits `emission_per_window` VRAM tokens, distributed by OpenSkill weight $w_i$. |
| `seal_policy.move` | `seal_approve` entry point — the IBE access gate. Seal key servers call this to decide if a validator may decrypt. |
| `vram_token.move` | VRAM coin definition. OTW pattern; `TreasuryCap` held by deployer, used only by `reward_distributor`. |

### Trust Model in Five Steps

1. **Miner registers** → `peer_registry.move` stores IBE-encrypted R2 credentials on-chain
2. **Validator decrypts** → Seal key servers simulate a PTB calling `seal_approve`; only staked, active validators get IBE key fragments
3. **Enclave evaluates** → gradient + checkpoint → loss delta → OpenSkill update → Ed25519 signature
4. **Score submitted** → `score_ledger.move` verifies signature against `enclave_registry.move`; cross-checks checkpoint hash against `round_state.move`
5. **Rewards emitted** → `reward_distributor.move` reads normalized weights $w_i$ from `score_ledger.move` and distributes tokens

---

## Rust Crate Map

```
crates/
  vramhub-core/        Shared types (PeerId, WindowId, VramhubError), OpenSkill impl
                       No I/O — imported by everything, depends on nothing internal.

  vramhub-chain/       SuiChainClient — one method per on-chain operation.
                       Handles PTB construction, Ed25519 keypair from mnemonic,
                       transaction submission and waiting.

  vramhub-seal/        Seal IBE client — encrypt/decrypt R2 credentials.
                       Session key derivation, threshold key server HTTP calls,
                       IBE key reconstruction from t-of-n fragments.

  vramhub-comms/       Cloudflare R2 client (S3-compatible HTTP).
                       Gradient upload/download, checkpoint storage,
                       FineWeb-edu shard assignment.

  vramhub-adapter/     TrainingFrameworkAdapter trait + implementations.
    src/sidecar.rs       Python HTTP sidecar (~200 lines, reference impl)
    src/candle_gpt.rs    Native Rust nano-GPT (6-layer, 384-dim, ~10M params)
    src/candle.rs        Simple MLP adapter (baseline)
    src/training.rs      Top-K f16 DCT gradient compression

  vramhub-miner/       Miner daemon binary.
    src/miner.rs         Main training loop: window → train → compress → upload → anchor
    src/config.rs        Env-var config loading (all VRAMHUB_* vars)

  vramhub-validator/   Validator daemon binary.
    src/validator.rs     Main validation loop: decrypt → download → enclave → submit
    src/attestation.rs   Nitro attestation document verification (COSE_Sign1)
    src/fast_eval.rs     Synchronous forward-pass scoring (VRAMHUB_TEST_MODE)

  vramhub-nautilus/    Nitro Enclave server binary.
                       HTTP server, loss evaluator, OpenSkill update,
                       Ed25519 signing via NSM. Runs inside the TEE.

  vramhub-aggregator/  Gradient aggregation every checkpoint_frequency windows.
                       Merges miner gradients, builds new checkpoint,
                       anchors checkpoint hash in round_state.move.

  vramhub-cli/         Operator CLI: register-enclave, register-miner, scores, status.

  vramhub-local-demo/  Full in-process simulation: 6 miners, 3 validators, toy LLM.
                       No wallet, no GPU, no cloud accounts. Runs in 5 minutes.
```

---

## VRAMScan — The Block Explorer

VRAMScan is the network's public-facing dashboard. It reads all state from Sui RPC in real-time — there is no backend; all data is on-chain.

```
vramscan/
  app/
    /                  Network overview: live window, peer counts, emission rate
    /miners            Miner leaderboard — OpenSkill weights, score history
    /validators        Validator list — stake, badge (NITRO ENCLAVE / SIMULATED)
    /windows           Historical window data — scores, emission per window
    /window/[id]       Single window detail — per-miner scores, gradient hashes
    /wallet/[address]  User dashboard — earnings, quest tracking
    /runs              Local demo run history with score progression charts
    /training          Earnings calculator, GPU pricing comparison, quick-start
    /join              Step-by-step onboarding for new miners and validators
    /docs/[[...slug]]  Protocol docs rendered from /docs directory

  lib/
    api-real.ts        All blockchain reads via Sui RPC (GraphQL + JSON-RPC)
    tokenomics.ts      VRAM emission schedule calculations
```

The validator badge (`NITRO ENCLAVE` vs `SIMULATED`) is set on-chain. Validators running with `VRAMHUB_TEST_MODE=true` produce simulated scores; mainnet contracts reject simulated scoring. The distinction is always visible on VRAMScan.

---

## Token Economics

| Parameter | Value |
|-----------|-------|
| Token | VRAM |
| Hard cap | 500,000,000 VRAM (9 decimals) |
| Window duration | 10 minutes |
| Windows per year | 52,560 |
| Genesis emission | 1,200 VRAM / window |
| Halving cadence | Supply-based: triggers at 126M and 189M mining tokens issued |

**Emission schedule (supply-based halvings):**

| Epoch | Mining tokens issued | Emission/Window | Mining allocation used |
|-------|---------------------|-----------------|----------------------|
| 1 | 0 → 126M | 1,200 VRAM | 0–46% |
| 2 | 126M → 189M | 600 VRAM | 46–69% |
| 3 | 189M → 275M | 300 VRAM | 69–100% |

Halving is triggered by cumulative mining issuance — not a timer. `current_emission_rate()` in `reward_distributor.move` is a pure function of `mining_tokens_issued`; there is no clock dependency.

**Per-window distribution (v0.5+):**

| Recipient | Share | Amount at Genesis |
|-----------|-------|------------------|
| Miners | 72% | 864 VRAM |
| Validators | 18% | 216 VRAM |
| Protocol treasury | 10% | 120 VRAM |

> **Testnet (current):** 100% to miners as contribution points. Miners who join within the first 90 days earn **2× points**. Points convert to VRAM at TGE via `(your_points / total_points) × 25,000,000`, capped at 2,500,000 VRAM per address. See [vramhub-points](../crates/vramhub-points) for the off-chain ledger.
>
> The 72/18/10 split ships in v0.5 alongside the validator reward module.

### Token Allocation

| Allocation | VRAM | % of Supply |
|------------|------|-------------|
| Mining rewards | 275,000,000 | 55% |
| Treasury | 100,000,000 | 20% |
| Team (1y cliff, 3y vest) | 40,000,000 | 8% |
| Liquidity | 35,000,000 | 7% |
| Community airdrop | 25,000,000 | 5% |
| GEM Digital | 25,000,000 | 5% |

Treasury and team tokens vest linearly after their cliff. The treasury cliff is 6 months; the team cliff is 12 months. All vesting is enforced on-chain in `vram_token.move`.

### Validator Bonding Curve

Entering as a validator requires a **permanent VRAM burn** (tokens sent to a no-withdrawal vault). The burn amount increases with each new validator registered, creating strong early-entry incentives:

| Validator slot | VRAM burned |
|---------------|-------------|
| 1 – 25 | 50,000 VRAM |
| 26 – 100 | 100,000 VRAM |
| 101 – 250 | 250,000 VRAM |
| 251 – 500 | 500,000 VRAM |

The `ValidatorTicket` issued on burn is **soulbound** (`has key`, no `has store`) — it cannot be transferred or sold. Burned VRAM is permanently removed from circulation; there is no withdrawal function in `validator_registry.move`.

### Anti-Gaming Properties

| Attack | Why It Fails |
|--------|-------------|
| Upload garbage gradient | Loss delta ≤ 0 → low OpenSkill score → near-zero reward weight |
| Copy another miner's gradient | Each miner is scored on their own assigned batch; a copied gradient for another miner's data produces near-zero delta on yours |
| Submit stale gradient | Checkpoint hash is checked; gradient computed from a stale checkpoint produces wrong loss |
| Forge validator signature | Enclave private key never leaves the TEE; forgery is computationally infeasible |
| Modify the enclave binary | Different PCRs → fails `register_enclave` → cannot submit scores |
| Sybil miners | Each peer requires stake; fake miners earn nothing |
| Collude with a key server | Threshold t-of-n: compromising one key server is insufficient to decrypt credentials |

---

## Validator Mode Quick Reference

| Mode | Set via | Enclave required | Signature | Mainnet valid | VRAMScan badge |
|------|---------|-----------------|-----------|--------------|----------------|
| Simulated | `VRAMHUB_TEST_MODE=true` | No | Zero (64×0x00) | No | SIMULATED |
| Dev | `VRAMHUB_NAUTILUS_URL=...` | Yes (local/dev) | Real, locally verified | No | NITRO ENCLAVE |
| Nitro | `VRAMHUB_NITRO_ENCLAVE=true` + `VRAMHUB_ENCLAVE_PUBKEY=...` | Yes (AWS c5+) | Real, on-chain pinned | Yes | NITRO ENCLAVE |

See the [Nautilus — The Three Modes](#nautilus--the-three-modes) section above for full details on each mode and when to use them.

---

## Gradient Lifecycle — End to End

```
Miner                      R2                    Validator              Chain
  │                         │                         │                    │
  │─── load checkpoint ─────┼─────────────────────────┼────────────────────┤
  │                         │                         │                    │
  │─── train_step(uid,w) ──►│                         │                    │
  │    (sidecar /train)     │                         │                    │
  │◄── { gradient, loss } ──│                         │                    │
  │                         │                         │                    │
  │─── DCT top-K compress   │                         │                    │
  │─── PUT gradient ───────►│                         │                    │
  │    key: grad-{w}-{uid}  │                         │                    │
  │─── anchor hash ─────────┼─────────────────────────┼───────────────────►│
  │                         │                         │                    │
  │                         │◄── decrypt R2 creds ────┤ (via Seal IBE)     │
  │                         │◄── GET gradient ────────┤                    │
  │                         │                         │                    │
  │                         │            send to enclave                   │
  │                         │         ┌──────────────────┐                 │
  │                         │         │  Nautilus TEE    │                 │
  │                         │         │  loss_before     │                 │
  │                         │         │  loss_after      │                 │
  │                         │         │  OpenSkill       │                 │
  │                         │         │  Ed25519 sign    │                 │
  │                         │         └──────────────────┘                 │
  │                         │                         │                    │
  │                         │                         │─── submit_scores ─►│
  │                         │                         │                    │── verify sig
  │                         │                         │                    │── record scores
  │                         │                         │                    │── emit rewards
  │                         │                         │                    │
```

---

## See Also

- [Incentive Mechanism](incentives.md) — full math for OpenSkill and emission schedule
- [Security Model](security.md) — attack surface analysis and limitations
- [Python Sidecar](miners/sidecar.md) — deep dive into the two-process miner architecture
- [Miner Setup](miners/setup.md) — wallet, R2, and testnet registration
- [Validator Setup](validators/setup.md) — Nitro enclave setup and PCR registration
- [Environment Variables](reference/env-vars.md) — all `VRAMHUB_*` configuration
- [ONBOARDING.md](../ONBOARDING.md) — 5-minute quick start
