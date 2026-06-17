# Vram Network

[![Sui Testnet](https://img.shields.io/badge/sui-testnet-72D900)](https://suiscan.xyz/testnet/object/0xaff18bf6286047126901610d758d8fd111c9215a6e46abc704b6a0be838badd5)
[![Move tests](https://img.shields.io/badge/move%20tests-101%2F101-72D900)](contracts/tests/)

```
▄▄▄▄  ▄▄▄▄ ▄▄▄▄▄▄▄     ▄▄▄▄   ▄▄▄      ▄▄▄   ▄▄▄   ▄▄▄ ▄▄▄  ▄▄▄ ▄▄▄▄▄▄▄
▀███  ███▀ ███▀▀███▄ ▄██▀▀██▄ ████▄  ▄████   ███   ███ ███  ███ ███▀▀███▄
 ███  ███  ███▄▄███▀ ███  ███ ███▀████▀███   █████████ ███  ███ ███▄▄███▀
 ███▄▄███  ███▀▀██▄  ███▀▀███ ███  ▀▀  ███   ███▀▀▀███ ███▄▄███ ███  ███▄
  ▀████▀   ███  ▀███ ███  ███ ███      ███   ███   ███ ▀██████▀ ████████▀
```

**Trustless decentralised machine learning on Sui.** GPU miners train models; AWS Nitro Enclave validators score every gradient and sign the proof on-chain; rewards settle in **VRAM** every 10 minutes — no trusted coordinator, no whitelist.

[**Try the live demo →**](https://www.vram.network/demotrain) (drag-drop a zip, upload to Walrus, run a simulated training) · [**Block explorer**](https://www.vram.network) · [**Whitepaper (NDA)**](mailto:team@vram.ai) · [**Sui Explorer**](https://suiscan.xyz/testnet/object/0xaff18bf6286047126901610d758d8fd111c9215a6e46abc704b6a0be838badd5)

---

## What this repo is

> **What this repo contains:** the working monorepo for Vram Network. Closed-source miner, validator, aggregator, and Nautilus enclave binaries are distributed from this repo via signed install scripts. The protocol itself — smart contracts, adapter framework, storage layer, SDK — is mirrored to the public repo [`VRAM-AI/vram-sdk`](https://github.com/VRAM-AI/vram-sdk) for builders.
>
> **This is the public front door for miners.** Run `curl -sSL https://www.vram.network/install.sh | sh`, set your wallet mnemonic in `~/.vramhub/.env`, and start earning VRAM. No GPU, no cloud account, no whitelist. The miner binary is closed-source; the network protocol it speaks is fully open and verifiable on chain.
>
> **For builders who want to compose on top of Vram Network** — post training jobs, plug a new model architecture into the SidecarAdapter, run inference against trained models — use [`VRAM-AI/vram-sdk`](https://github.com/VRAM-AI/vram-sdk) (open SDK + contracts + docs).

This split mirrors the Claude Code pattern: the wrapper, the protocol, and the SDKs are open; the engine binaries are closed but freely installable.

---

## Run a miner (closed-source binary, freely installable)

One line on Linux / macOS:

```bash
curl -sSL https://www.vram.network/install.sh | sh
```

The script:
- downloads a signed `vram-miner` binary (Apple Silicon, Linux x86_64, Linux ARM64)
- writes a `~/.vramhub/.env` template
- prints next-step instructions

Then:

```bash
# Set wallet mnemonic
$EDITOR ~/.vramhub/.env

# Start mining
vram-miner
```

**Why run now (testnet pre-TGE):**

| Field | Value |
|---|---|
| Network | Sui Testnet (chain-id `4c78adac`) |
| Reward pool | 6,000,000 VRAM minted, ready to distribute |
| Emission | 70 VRAM / 10-minute window |
| Min stake | 1 SUI (refundable) |
| Airdrop conversion | testnet contribution-points → real VRAM at TGE |
| Early bonus | 2× active until 90 days after TGE snapshot |

---

## Roadmap

| Feature | Status |
|---------|--------|
| Miner binary (testnet) | ✅ Live |
| Validator (test mode) | ✅ Live |
| Validator (Nitro enclave, mainnet) | ⏳ In progress |
| VRAM token payouts | ⏳ Needs first Nitro validator |
| `vram-sdk` (builders API) | 🗓 Q3 2026 |
| **GPU marketplace / rental** | 🗓 Planned — rent GPU capacity on-demand without running a full miner node |

---

## Build on top of Vram Network

If you want to **post jobs**, **run inference**, or **plug a new model architecture** — wait for `vram-sdk` (Q3 2026) or contact `team@vram.ai` for early-access.

In the meantime:

| You want to | Use |
|---|---|
| Run the protocol locally (smoke test) | `cargo run --bin vramhub-local-demo` (closed source until vram-sdk launches) |
| Try the customer flow (post a training job) | https://www.vram.network/demotrain |
| Verify the contracts | https://suiscan.xyz/testnet/object/0xaff18bf6286047126901610d758d8fd111c9215a6e46abc704b6a0be838badd5 |
| Snapshot the airdrop counter | `python scripts/snapshot_airdrop.py` |

---

## v0.7 testnet contracts (live)

| Object | Address | Explorer |
|---|---|---|
| **Package** | `0xaff18bf6286047126901610d758d8fd111c9215a6e46abc704b6a0be838badd5` | [suiscan](https://suiscan.xyz/testnet/object/0xaff18bf6286047126901610d758d8fd111c9215a6e46abc704b6a0be838badd5) |
| PeerRegistry | `0x50a9982f6a3d6c1e6674f0fb4fa8b985007dbe19fc797abc691579be1f6493df` | [suiscan](https://suiscan.xyz/testnet/object/0x50a9982f6a3d6c1e6674f0fb4fa8b985007dbe19fc797abc691579be1f6493df) |
| ValidatorRegistry | `0x438d0ce63d40210d4e621cca6aaaf5d5438adaa54dfd71383fe41a82692a2561` | [suiscan](https://suiscan.xyz/testnet/object/0x438d0ce63d40210d4e621cca6aaaf5d5438adaa54dfd71383fe41a82692a2561) |
| EnclaveRegistry | `0x442b82e471c1ee4577ea1f2168deb1f0b04fcc861ab79edb4b9c7d7738bf7f9f` | [suiscan](https://suiscan.xyz/testnet/object/0x442b82e471c1ee4577ea1f2168deb1f0b04fcc861ab79edb4b9c7d7738bf7f9f) |
| ScoreLedger | `0x0d2594727abeb45a13763baf8801ae765fbe41d147b28916ca78a0d08f73223a` | [suiscan](https://suiscan.xyz/testnet/object/0x0d2594727abeb45a13763baf8801ae765fbe41d147b28916ca78a0d08f73223a) |
| RoundState | `0xc1f18dc92629907641bc3176449af39738d2d8a93b4ad6b22548f4aed91d2611` | [suiscan](https://suiscan.xyz/testnet/object/0xc1f18dc92629907641bc3176449af39738d2d8a93b4ad6b22548f4aed91d2611) |
| Hparams | `0x18b884530033f9b3e449b898c540ee5d3a25c4cab0abcf4843ef8e86e12adbfc` | [suiscan](https://suiscan.xyz/testnet/object/0x18b884530033f9b3e449b898c540ee5d3a25c4cab0abcf4843ef8e86e12adbfc) |
| RewardPool | `0x576ebeb78449ad46ef70dc3c5ca4e38d178846610bd7cf9f0764ae2f1dc0fe93` | [suiscan](https://suiscan.xyz/testnet/object/0x576ebeb78449ad46ef70dc3c5ca4e38d178846610bd7cf9f0764ae2f1dc0fe93) |
| TrainingJobBoard | `0xb481254350087569f904fe6fc45c337c0905651040791e532e0f044b9fc7474c` | [suiscan](https://suiscan.xyz/testnet/object/0xb481254350087569f904fe6fc45c337c0905651040791e532e0f044b9fc7474c) |

Chain-id `4c78adac`. Deployer wallet [`0xb7aaeb31…c772c3`](https://suiscan.xyz/testnet/account/0xb7aaeb31d576814e1b268a43033feccac19a2905a652ad3b42fb5efeb1c772c3).

---

## Architecture in one diagram

```
   GPU Miner ──── compressed gradient ───→  Walrus (Sui)
                                                 │
                                                 ▼
                                            Validator (AWS Nitro Enclave)
                                                 │
                                          attested loss-delta
                                                 ▼
                                         Sui ScoreLedger
                                                 │
                                       Bayesian OpenSkill update
                                                 ▼
                                         RewardDistributor
                                                 │
                                          VRAM per window
                                                 ▼
                                              Miner wallet
```

See [docs/architecture.md](docs/architecture.md) for the full diagram + threat model.

---

## Tokenomics (v0.7 canonical — matches `vram_token.move` on chain)

- **Hard cap**: 21,000,000 VRAM
- **Mining pool (50%)**: 10,500,000 VRAM — emitted per-window by `reward_distributor` with a supply-based halving. Tracked by `MINING_ALLOCATION` in `hparams`.
- **TGE pre-mint (50%)**: 10,500,000 VRAM — minted once at TGE in a single atomic call:
  - Treasury 30% / 6,300,000 VRAM — 6m cliff, 48m linear vest
  - Team 8% / 1,680,000 VRAM — 12m cliff, 36m linear vest
  - Liquidity 7% / 1,470,000 VRAM — 100% unlocked, seeds VRAM/SUI pool on Cetus
  - Airdrop 5% / 1,050,000 VRAM — instant at TGE, converts from testnet contribution points
- **Halving schedule** (supply-based, halts at 10.5M mining tokens issued):
  - Phase 1: 70 VRAM/window — from 0 to 7M mining tokens issued
  - Phase 2: 35 VRAM/window — from 7M to 10.5M
  - Halts at 10.5M (the `MINING_ALLOCATION` cap)
- **Validator bonding curve** (max 500 slots): 2,100 → 4,200 → 10,500 → 21,000 VRAM burned per slot across 4 tiers. At full capacity ~7.2M VRAM is permanently locked in `validator_registry.burn_vault`. ValidatorTicket is soulbound.
- **TGE price** (illustrative): $0.71 per VRAM

---

## License

- **Smart contracts** (`contracts/`) — MIT
- **Open SDK crates** (mirrored to vram-sdk) — Apache 2.0
- **Closed daemons** (`vramhub-miner`, `vramhub-validator`, `vramhub-aggregator`) — proprietary; runnable binaries distributed under [terms in `LICENSE-BINARIES.md`](LICENSE-BINARIES.md)
- **Paper** — copyright VRAM AI Limited; not released until v0.9 transformer convergence study completes

See [SECURITY.md](SECURITY.md) for the responsible-disclosure policy and bug bounty.

---

## Contact

- **Team**: team@vram.ai
- **Builders**: builders@vram.ai (until vram-sdk launches, this is the early-access channel)
- **Security**: security@vram.ai (PGP key in [SECURITY.md](SECURITY.md))

© 2024-2026 VRAM AI Limited.
