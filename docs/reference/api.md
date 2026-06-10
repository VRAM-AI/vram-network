# API Reference

## Nautilus Enclave HTTP API

The Nautilus enclave exposes an HTTP server on port 3000. All requests and responses are JSON.

### `GET /health_check`

Returns the enclave's health status.

**Response:**
```
ok
```

---

### `GET /get_attestation`

Returns the Nitro attestation document and enclave public key. Used by `vramhub-cli register-enclave`.

**Response:**
```json
{
  "attestation_document": "a1820184...",
  "public_key": "a3f4b2c1..."
}
```

- `attestation_document` — hex-encoded COSE_Sign1 CBOR structure containing PCR0, PCR1, PCR2, and the public key, signed by the AWS Nitro root CA
- `public_key` — hex-encoded Ed25519 public key (32 bytes)

---

### `POST /process_data`

Evaluates gradients for a window and returns signed scores.

**Request:**
```json
{
  "window": 2957300,
  "miner_uid": 42,
  "gradient_bytes": "aabbcc...",
  "checkpoint_hash": "deadbeef..."
}
```

- `gradient_bytes` — hex-encoded compressed gradient (DCT top-k format)
- `checkpoint_hash` — hex-encoded SHA-256 hash of the current checkpoint

**Response:**
```json
{
  "score": 142857000,
  "signed_payload": "cafebabe...",
  "signature": "112233..."
}
```

- `score` — loss delta in 1e9 fixed-point (positive = miner improved the model)
- `signed_payload` — hex-encoded CBOR payload that was signed: `(window, checkpoint_hash, {miner_uid: score})`
- `signature` — hex-encoded Ed25519 signature (64 bytes) over `signed_payload`

The validator submits `signed_payload` and `signature` to `score_ledger.move` via `submit_scores`.

---

## CLI Commands

The `vramhub-cli` binary provides operator commands:

### `register-enclave`

```bash
cargo run --bin vramhub-cli -- register-enclave \
  --enclave-url http://<EC2_IP>:3000 \
  --validator-uid <UID>
```

Fetches the Nitro attestation from the enclave, extracts PCR values, and submits `register_enclave` on-chain.

---

### `scores`

```bash
cargo run --bin vramhub-cli -- scores --window <WINDOW>
# or
cargo run --bin vramhub-cli -- scores --uid <MINER_UID>
```

Reads scores from `score_ledger.move` for a given window or miner.

---

### `status`

```bash
cargo run --bin vramhub-cli -- status
cargo run --bin vramhub-cli -- status --uid <UID>
cargo run --bin vramhub-cli -- status --validator-uid <UID>
```

Prints current window, registered peers, and recent activity.

---

### `checkpoint`

```bash
cargo run --bin vramhub-cli -- checkpoint
cargo run --bin vramhub-cli -- checkpoint --window <WINDOW>
```

Shows checkpoint info (hash, R2 path, finalization status) for the current or a specified window.

---

## Rust Crate API

The primary programmatic interface is `vramhub-chain`'s `SuiChainClient`:

```rust
use vramhub_chain::SuiChainClient;

let client = SuiChainClient::from_env().await?;

// Read hparams
let hparams = client.get_hparams().await?;

// Get current window
let window = client.current_window().await?;

// Submit scores
client.submit_scores(
    validator_uid,
    window,
    scores_keys,
    scores_values,
    stake,
    submitted_at_ms,
    enclave_signature,
    signed_payload_bytes,
    checkpoint_hash,
    expected_checkpoint_hash,
).await?;
```

See `crates/vramhub-chain/src/` for the full method list.
