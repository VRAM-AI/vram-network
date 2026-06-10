# Introduction

VRAM HUB is a decentralized training coordination layer that enables large-scale LLM pre-training across heterogeneous compute resources over the internet. Built on the Sui blockchain, it replaces trusted coordinators with verifiable on-chain incentives, Seal IBE credential encryption, and Nitro TEE-attested loss evaluation.

## What Problem Does VRAM HUB Solve?

Large-scale LLM pre-training traditionally requires:

1. **Centralized orchestration** — a trusted coordinator routes data, collects gradients, and distributes rewards
2. **Homogeneous compute** — training clusters assume identical hardware and network conditions
3. **Trust in evaluators** — a centralized party determines who contributed useful work

VRAM HUB eliminates all three requirements:

- Every coordination event (window, score, reward) is anchored on **Sui** — verifiable by anyone
- **Heterogeneous compute** is first-class: miners can run any hardware in any geography
- **Nitro TEE attestation** makes loss evaluation unforgeable — the enclave's binary is committed to on-chain PCR values; a compromised scorer fails registration

## Core Participants

| Role | Responsibility |
|------|---------------|
| **Miner** | Runs GPU compute; trains on assigned data; uploads compressed gradient to Cloudflare R2 |
| **Validator** | Decrypts miner credentials via Seal; evaluates gradients inside a Nautilus enclave; submits signed scores on-chain |
| **Aggregator** | Merges gradients each `checkpoint_frequency` windows; anchors checkpoint hash on-chain |

## How Windows Work

The system advances in synchronized **windows** (default: 10 minutes each):

```
Window N begins
  ├── Miners load checkpoint from window N-1
  ├── Miners train on their assigned data batch
  ├── Miners upload compressed gradient to R2
  │     └── key: gradient-{window}-{uid}-v{version}.pt
  ├── Validators decrypt miner R2 credentials via Seal
  ├── Validators download gradients → send to Nautilus enclave
  ├── Enclave computes loss delta → signs result with Ed25519
  ├── Validators submit signed scores to score_ledger.move
  └── reward_distributor.move emits tokens weighted by OpenSkill rating
Window N+1 begins
```

## Key Properties

- **Permissionless** — any node with stake can register as miner or validator; no whitelist
- **Sybil-resistant** — rewards require stake; fake miners without stake earn nothing
- **Verifiable** — enclave PCR values committed on-chain; any observer can verify scorer integrity
- **Pluggable** — bring your own training framework via the `TrainingFrameworkAdapter` trait

## Technology Stack

| Layer | Technology |
|-------|-----------|
| Blockchain | Sui (Move smart contracts) |
| Credential privacy | Sui Seal IBE (threshold t-of-n key servers) |
| Score integrity | AWS Nitro Enclaves (Nautilus) |
| Gradient transport | Cloudflare R2 (S3-compatible) |
| Skill rating | OpenSkill Plackett-Luce |
| Node software | Rust (async, Tokio) |
| Block explorer | VRAMScan — Next.js 14 (http://localhost:4322) |
