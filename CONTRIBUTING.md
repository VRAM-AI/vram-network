# Contributing to Vram Network

Thank you for your interest.

> **Project split.** This repository (`VRAM-HUB`) is the working monorepo for Vram Network. It contains the closed-source miner / validator / aggregator daemons (distributed as signed binaries) and the protocol source. The public builder SDK — open smart contracts, adapter framework, docs, and reference sidecars — is mirrored to **github.com/VRAM-AI/vram-sdk** (launching Q3 2026). That's the right home for most external contributions; the rules below describe what we accept here.

---

## What's accepted from external contributors

| Component | Status | License |
|---|---|---|
| Smart contracts (`contracts/sources/*.move`) | **OPEN** | MIT |
| Adapter trait + reference adapters (`crates/vramhub-adapter/`) | **OPEN** | Apache 2.0 |
| Sui RPC client (`crates/vramhub-chain/`) | **OPEN** | Apache 2.0 |
| Operator CLI (`crates/vramhub-cli/`) | **OPEN** | Apache 2.0 |
| Block explorer (`vramscan/`) | **OPEN** | MIT |
| Documentation (`docs/`) | **OPEN** | CC BY 4.0 |
| Installer scripts (`scripts/install*.sh`) | **OPEN** | MIT |
| Reference sidecars (`scripts/yolo_trainer.py`, `scripts/vram_trainer.py`) | **OPEN** | MIT |
| **Miner / Validator / Aggregator daemons** | **CLOSED** — proprietary | not accepting PRs; signed binaries distributed via release channel |
| **Enclave EIF** (lives in github.com/0x0sid/slc-nautilus) | **CLOSED** | not accepting PRs |
| **Paper** (`paper/`) | **PRIVATE** until v0.9 transformer study completes | email team@vram.ai for review access |

---

## Ways to contribute

| Type | Where to start |
|------|---------------|
| Bug reports | Open a GitHub issue with steps to reproduce |
| New training adapters | See [Adding an Adapter](#adding-a-training-adapter) below |
| Smart contract improvements | See [Contract Changes](#smart-contract-changes) |
| VRAMScan UI | See [Frontend](#vramscan-frontend) |
| Documentation | Any `.md` file in `docs/` |
| Running a miner | See [ONBOARDING.md](ONBOARDING.md) |
| Running a validator | See [docs/validators/setup.md](docs/validators/setup.md) |

---

## Development Setup

### Prerequisites

- Rust 1.80+ (`rustup update stable`)
- Node.js 20+ (for VRAMScan)
- Python 3.10+ (for the sidecar trainer)

### Quick start — local demo (no wallet, no GPU)

```bash
git clone https://github.com/VRAM-AI/VRAM-HUB.git
cd VRAM-HUB

# Run full local simulation (6 miners, 3 validators, toy LLM)
cargo run -p vramhub-local-demo

# In a second terminal — block explorer
cd vramscan && npm install && npm run dev
# → http://localhost:4322
```

### Running tests

```bash
# Rust unit tests
cargo test --workspace

# Move contract tests (requires Sui CLI)
cd contracts && sui move test

# VRAMScan
cd vramscan && npm test
```

---

## Project Structure

```
contracts/         Sui Move smart contracts
crates/
  vramhub-core/       Shared types, OpenSkill, error types — no I/O
  vramhub-chain/      Sui RPC client (one function per on-chain call)
  vramhub-comms/      Cloudflare R2 client, dataset loader
  vramhub-seal/       Seal IBE encryption/decryption
  vramhub-adapter/    Training framework adapters (trait + impls)
  vramhub-miner/      Miner daemon binary
  vramhub-validator/  Validator daemon binary
  vramhub-nautilus/   Nitro Enclave server (TEE scoring)
  vramhub-aggregator/ Gradient aggregation, checkpoint building
  vramhub-cli/        Operator CLI (register, status, scores)
  vramhub-local-demo/ Full in-process simulation for local testing
vramscan/          Next.js block explorer
scripts/           Python sidecar trainer, enclave build scripts
docs/              Protocol documentation
paper/             Academic paper (LaTeX + PDF)
```

---

## Adding a Training Adapter

The adapter system is the cleanest extension point. An adapter wraps any training framework and makes it work as a VRAM miner.

**Implement the trait** in `crates/vramhub-adapter/src/`:

```rust
use vramhub_adapter::TrainingFrameworkAdapter;
use vramhub_core::VramhubError;

pub struct MyAdapter { /* your state */ }

#[async_trait]
impl TrainingFrameworkAdapter for MyAdapter {
    fn name(&self) -> &str { "my-adapter" }
    async fn load_checkpoint(&mut self, data: &[u8]) -> Result<(), VramhubError> { ... }
    async fn train_step(&mut self, batch: &[i64]) -> Result<Vec<f32>, VramhubError> { ... }
    fn compress_gradient(&self, raw: &[f32]) -> Result<CompressedGradient, VramhubError> { ... }
    async fn save_checkpoint(&self) -> Result<Checkpoint, VramhubError> { ... }
    async fn forward_loss(&self, batch: &[i64]) -> Result<f32, VramhubError> { ... }
    async fn apply_gradient(&mut self, gradient: &[f32], beta: f32) -> Result<(), VramhubError> { ... }
}
```

Add a feature flag in `crates/vramhub-adapter/Cargo.toml`:

```toml
[features]
my-adapter = ["dep:your-crate"]
```

Register it in `crates/vramhub-miner/src/main.rs` under the feature-gated adapter selection block.

See `crates/vramhub-adapter/src/sidecar.rs` for the simplest reference implementation (Python HTTP sidecar — ~150 lines).

---

## Smart Contract Changes

All contracts are in `contracts/sources/`. The contracts are deployed to testnet; changes require redeployment.

Before opening a PR that touches contracts:

1. Run `sui move test` — all 7 tests must pass
2. Document the new object IDs in `.env.example` if new shared objects are added
3. If the change affects the scoring or reward logic, update `docs/incentives.md`

**Do not** change `seal_approve` in `seal_policy.move` without understanding the Seal IBE trust model (see `docs/security.md`).

---

## VRAMScan Frontend

The block explorer lives in `vramscan/`. It's a standard Next.js 14 app.

```bash
cd vramscan
cp .env.example .env.local   # fill in your values
npm install
npm run dev
```

All live blockchain data comes from `lib/api-real.ts` via Sui RPC — no mock data. If you're adding a new page, use the existing `getMiners()` / `getValidators()` / `getWindows()` functions as patterns.

Design system: black background (`#000000`), lime accent (`#72D900`), League Gothic for headings, IBM Plex Mono for data. All tokens are in `app/globals.css` as CSS variables (`--vram-acc`, `--vram-muted`, etc.) — use those, not hardcoded hex values.

---

## Code Style

- **Rust:** `cargo fmt` + `cargo clippy --workspace` before submitting. Fix all clippy warnings.
- **TypeScript:** Prettier with the project's existing config.
- **Move:** Follow the existing module structure; no `public(friend)` without justification.
- **Comments:** Add doc comments (`///`) on all public functions. Add inline comments on non-obvious logic — especially cryptographic operations.

---

## Pull Request Process

1. Fork the repo, create a branch from `master`
2. Make your change, add tests where applicable
3. Run `cargo test --workspace` and `sui move test`
4. Open a PR with a description of what changed and why
5. Link any relevant issues

---

## Security Issues

**Do not open public GitHub issues for security vulnerabilities.**

Email security@vram.ai or see [SECURITY.md](SECURITY.md) for the responsible disclosure process.

---

## License

Apache 2.0. By contributing, you agree that your contributions will be licensed under the same terms.
