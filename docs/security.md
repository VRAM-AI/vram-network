# Security Model

VRAM HUB's security rests on three independent mechanisms. Compromising the system requires breaking all three simultaneously.

## 1. Seal IBE — Credential Privacy

Miner R2 read credentials are encrypted using **Sui Seal's Identity-Based Encryption** before being stored on-chain. Only validators that pass the `seal_approve` check can decrypt them.

### How It Works

```
Miner registers:
  credentials = IBE_encrypt(R2_access_key + R2_secret, identity=validator_set)
  peer_registry.move stores ciphertext on-chain

Validator decrypts:
  1. Constructs a PTB that calls seal_approve in seal_policy.move
  2. Sends PTB to Seal key servers
  3. Key servers simulate the PTB — if seal_approve passes, they release IBE key fragments
  4. Validator reconstructs IBE key from t-of-n fragments
  5. Validator decrypts R2 credentials
```

### `seal_approve` Checks

`seal_policy.move` verifies all of the following before releasing key fragments:

- Caller is registered in `validator_registry.move`
- Caller has stake ≥ `min_validator_stake`
- Caller is marked active

### Threshold Security

The IBE key is split across **n** key servers; **t** fragments are sufficient to reconstruct it. A single compromised key server cannot leak credentials. As long as fewer than **t** servers are malicious, credentials remain private.

No trusted third party holds the master key — the threshold scheme distributes trust across the key server set.

## 2. Nitro TEE — Score Integrity

Loss evaluation runs inside an **AWS Nitro Enclave**, a hardware-isolated virtual machine. The enclave's software identity is committed to PCR (Platform Configuration Register) values measured at boot.

### Enclave Registration (One-Time, Expensive)

```
Enclave boots:
  1. Generates ephemeral Ed25519 keypair (private key never leaves enclave)
  2. Requests Nitro attestation document from NSM (Nitro Security Module)
     - Attestation includes: PCR0, PCR1, PCR2, enclave public key
     - PCRs are SHA-384 hashes of: OS image, application binary, application config
  3. Returns attestation document + public key via HTTP

Operator registers on-chain:
  cargo run --bin vramhub-cli -- register-enclave --enclave-url http://<EC2>:3000
  ↓
  register_enclave in enclave_registry.move:
    - Verifies the Nitro attestation document (full COSE_Sign1 verification)
    - Checks PCR0/PCR1/PCR2 match values stored in hparams.move
    - Records enclave public key
```

### Score Submission (Per-Window, Cheap)

```
Enclave signs:
  payload = CBOR(window, checkpoint_hash, {uid → score})
  signature = Ed25519_sign(payload, ephemeral_private_key)

Validator submits:
  score_ledger.move:
    1. Checks enclave_registry.move for registered enclave
    2. Verifies Ed25519_verify(signature, enclave_pubkey, payload)
    3. Checks checkpoint_hash matches round_state.move
    4. Records scores
```

This is **one signature verification per window** — cheap because the expensive attestation verification happened once at registration.

### PCR Binding

PCR values are stored in `hparams.move`. A modified or compromised enclave binary produces different PCR values and fails the registration check. Validators cannot substitute their own scorer.

### What the Enclave Cannot Do

- Lie about loss values without being detected (the signed payload is deterministic given the gradient and checkpoint)
- Operate with a different binary (PCRs would mismatch)
- Leak the signing key (private key was generated inside the enclave and never exported)

## 3. Sybil Resistance — Stake-Gated Participation

Both miners and validators must stake SUI to participate:

| Role | Minimum Stake |
|------|--------------|
| Miner | `min_miner_stake` (default: 1 SUI) |
| Validator | `min_validator_stake` (default: 10 SUI) |

Creating many fake miners without stake produces zero rewards. Creating fake validators without stake means they cannot pass `seal_approve` and cannot decrypt miner credentials.

## Attack Surface Analysis

| Attack | Mitigation |
|--------|-----------|
| Miner uploads garbage gradient | Loss delta will be zero or negative; miner gets low OpenSkill score → low rewards |
| Validator fakes scores | Cannot forge Ed25519 signature without access to enclave private key |
| Enclave binary modified | Different PCRs → fails `register_enclave`; cannot submit scores |
| Rogue key server leaks IBE key | Threshold scheme: t-of-n required; single server compromise insufficient |
| Sybil miners | Rewards require stake; fake miners earn nothing |
| Sybil validators | `seal_approve` requires stake; fake validators cannot decrypt credentials |
| Checkpoint manipulation | Checkpoint hash anchored on-chain in `round_state.move`; `score_ledger.move` cross-checks |

## What Is Not Covered

- **Data quality**: Miners train on deterministically assigned data, but the dataset itself is not verified on-chain
- **Gradient correctness**: The enclave verifies loss improvement, not that the gradient was computed correctly; a lucky random gradient would still score
- **Network-level attacks**: DoS on R2 buckets or Seal key servers is out of scope; operators should use standard cloud mitigations
