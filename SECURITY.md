# Security Policy

## Supported Versions

| Version | Network | Status |
|---|---|---|
| v0.7.x | **Sui Testnet** — package `0xaff18bf6…badd5` | **Active.** Security findings accepted and rewarded (see bounty below). |
| v0.6.x | Sui Testnet — package `0x48703e08…db0aa8` (deprecated) | Out of scope; redeploy already happened. |
| Earlier | — | Not supported |

Vram Network is currently in **testnet**. All contracts, token balances, and miner scores are on Sui Testnet and have no real monetary value. The codebase is moving toward mainnet during the May–June 2026 TGE window; **security findings now are the most valuable** — they harden the system before real value is at risk.

---

## Reporting a Vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Email: **security@vram.ai**

Include:
- Description of the vulnerability
- Steps to reproduce
- Affected component (contracts, enclave, miner, validator, aggregator, VRAMScan, SDK)
- Your assessment of the severity (low / medium / high / critical)
- PGP public key if you want our reply encrypted

We acknowledge receipt within **48 hours** and aim to publish a fix or mitigation within **14 days** for critical issues. Public disclosure happens after the fix lands, with credit to the reporter unless anonymity is requested.

---

## Bug Bounty (placeholder — formal program launches at TGE)

Pre-TGE: discretionary rewards in testnet VRAM convertible at TGE.

Post-TGE planned tiers (subject to change):

| Severity | Example | Reward range |
|---|---|---|
| Critical | Funds-loss bug in `reward_distributor` / `vram_token`; enclave attestation bypass | up to $50,000 in VRAM |
| High | Score forgery against ScoreLedger; bonding-curve bypass | up to $15,000 in VRAM |
| Medium | DoS against per-window aggregation; partial unauthorised state read | up to $5,000 in VRAM |
| Low | Vramscan XSS / open-redirect; minor docs / security oversights | up to $1,000 in VRAM |

Scope intentionally **excludes** social engineering, physical attacks on validator hosts, and known issues already published in `contracts/MAINNET_CHANGES.md`.

---

## Security Model Summary

Understanding the trust model helps you find the boundaries worth probing.

### What is trusted

| Component | Trust basis |
|-----------|-------------|
| Sui blockchain | Public ledger — all on-chain state is verifiable |
| AWS Nitro Enclave | Hardware attestation — PCR0/PCR1/PCR2 measurements registered on-chain |
| Seal IBE key servers | Mysten Labs testnet key servers — 2-of-2 threshold |
| Ed25519 signatures | Enclave hardware key (not extractable) |

### What is NOT trusted

| Component | Why |
|-----------|-----|
| Miners | May submit invalid gradients or lie about training |
| Validators (outside TEE) | May attempt to forge scores |
| R2 storage | Cloud storage — contents encrypted at rest via Seal IBE before upload |
| VRAMScan UI | Read-only block explorer — no signing authority |

### Key security boundaries

**Proof-of-Gradient-Quality (PoGQ):**
The enclave downloads the gradient from R2, computes `loss_before - loss_after` on a validation batch, signs the result with a hardware key whose public key is registered on-chain. The `score_ledger` contract verifies this signature before accepting a score. Forging a score requires breaking Ed25519 or compromising the Nitro hardware.

**Seal IBE credential privacy:**
Miner R2 credentials (bucket name, access key, secret key) are encrypted with Seal IBE before on-chain registration. Only validators who have registered stake and pass `seal_approve` can obtain the decryption key. This prevents unauthenticated access to miner storage.

**Deterministic data assignment:**
`seed = SHA256(uid ‖ window)` — the assignment is public and reproducible. Validators use the same seed to verify that miners trained on the correct data.

---

## Known Limitations (Testnet)

These are known gaps being addressed before mainnet:

1. **`VRAMHUB_SKIP_SEAL=true` in testnet** — The `.env.example` defaults to `VRAMHUB_SKIP_SEAL=true`, which bypasses Seal IBE encryption of R2 credentials. This is intentional for testnet ease-of-use. It **must be set to `false`** in any production deployment.

2. **Nitro root CA validation incomplete** — The validator currently trusts the enclave's Ed25519 signature without fully verifying the Nitro attestation document's certificate chain back to AWS root CA. This is tracked in `crates/vramhub-validator/src/attestation.rs` with a `// TODO` comment. The PCR-based registration mitigates this for now.

3. **No rate limiting on score submissions** — A validator can submit multiple scores per window. The contract accepts the first and ignores duplicates, but the lack of a per-window rate limit is a denial-of-service vector for gas costs.

4. **Supabase keys in VRAMScan** — The VRAMScan block explorer optionally uses a Supabase database for quest tracking and user profiles. If you deploy VRAMScan, rotate the default credentials in `.env.local` — never commit real credentials to the repo.

---

## Dependency Security

The Rust crates use well-audited cryptographic primitives:
- `ed25519-dalek` — Ed25519 signing/verification
- `bls12_381` — BLS12-381 pairing (IBE)
- `sha2` — SHA-256/SHA-384 hashing
- `aes-gcm` — AES-256-GCM authenticated encryption

Run `cargo audit` before deploying to check for known CVEs in dependencies.

---

## Responsible Disclosure History

No public disclosures to date. The project is pre-mainnet.
