# Environment Variables

All VRAM HUB binaries are configured via environment variables. Copy `.env.example` to `~/.vramhub/.env` and fill in the required values.

## Required for All Nodes

| Variable | Description |
|----------|-------------|
| `VRAMHUB_SUI_RPC_URL` | Sui RPC endpoint. Testnet default: `https://fullnode.testnet.sui.io:443` |
| `VRAMHUB_WALLET_MNEMONIC` | 12 or 24 word BIP-39 mnemonic for your Sui wallet |
| `VRAMHUB_PACKAGE_ID` | Deployed Move package object ID |
| `VRAMHUB_PEER_REGISTRY_ID` | `PeerRegistry` shared object ID |
| `VRAMHUB_ENCLAVE_REGISTRY_ID` | `EnclaveRegistry` shared object ID |
| `VRAMHUB_SCORE_LEDGER_ID` | `ScoreLedger` shared object ID |
| `VRAMHUB_ROUND_STATE_ID` | `RoundState` shared object ID |
| `VRAMHUB_HPARAMS_ID` | `Hparams` shared object ID |
| `VRAMHUB_REWARD_POOL_ID` | `RewardPool` shared object ID |

## Mode Flags (SLCL_ prefix)

The validator binary reads mode flags from the `SLCL_` prefix. Both `VRAMHUB_` and `SLCL_` must be set for these to work correctly.

| Variable | Description | Default |
|----------|-------------|---------|
| `SLCL_TEST_MODE` / `VRAMHUB_TEST_MODE` | Simulated scoring — no Nitro enclave required | `false` |
| `SLCL_NITRO_ENCLAVE` / `VRAMHUB_NITRO_ENCLAVE` | Enable Nitro enclave scoring (production) | `false` |
| `SLCL_SKIP_SEAL` / `VRAMHUB_SKIP_SEAL` | Skip Seal IBE credential encryption (testnet only) | `false` |
| `SLCL_VALIDATOR_UID` / `VRAMHUB_VALIDATOR_UID` | Your registered validator UID | — |

> **Tip:** Set both the `SLCL_` and `VRAMHUB_` versions in your `.env` for safety:
> ```bash
> VRAMHUB_TEST_MODE=true
> SLCL_TEST_MODE=true
> ```

## Miner-Only

| Variable | Description | Required |
|----------|-------------|----------|
| `VRAMHUB_MINER_UID` | Your registered peer UID. **Optional** — auto-registered on first startup and saved to `.vramhub-uid` | No |
| `VRAMHUB_STORAGE_BACKEND` | Storage backend for gradient uploads. Set to `walrus` (free on testnet) | Yes |
| `VRAMHUB_DEMO_MODE` | Enable demo/testnet mode (skips production storage checks) | Yes (testnet) |
| `VRAMHUB_WALRUS_PUBLISHER` | Walrus publisher endpoint (default: testnet publisher) | No |
| `VRAMHUB_WALRUS_AGGREGATOR` | Walrus aggregator endpoint (default: testnet aggregator) | No |
| `VRAMHUB_BATCH_SIZE` | Training batch size (default: `4` for nano-GPT) | No |
| `VRAMHUB_DEVICE` | Training device override: `cpu`, `cuda`, `cuda:0`, `cuda:1`, `metal` (default: auto-detect) | No |
| `VRAMHUB_SIDECAR_URL` | Python sidecar URL when using `--features sidecar` (default: `http://127.0.0.1:17070`) | No |

## Validator-Only

| Variable | Description | Required |
|----------|-------------|----------|
| `VRAMHUB_VALIDATOR_UID` / `SLCL_VALIDATOR_UID` | Your registered validator UID (from `vram-cli register-validator`) | Yes |
| `VRAMHUB_NAUTILUS_URL` | Enclave HTTP endpoint (default: `http://localhost:3000`) | Yes (Nitro mode) |
| `VRAMHUB_SEAL_KEY_SERVER_IDS` | Comma-separated Seal key server object IDs | No (testnet) |
| `VRAMHUB_SEAL_THRESHOLD` | Seal key server threshold t-of-n (default: `2`) | No |

## Testnet / Points Tracker

| Variable | Description | Required |
|----------|-------------|----------|
| `VRAMHUB_TESTNET_MODE` | Set to `true` to enable testnet-specific behaviour | No |
| `VRAMHUB_POINTS_API_URL` | URL of the `vramhub-points` REST API (default: `http://localhost:8080`) | No |

## Testnet Object IDs (v0.7)

All values are pre-filled in `.env.example`. Only `VRAMHUB_WALLET_MNEMONIC` needs to be set.

| Object | ID |
|--------|----|
| Package | `0xaff18bf6286047126901610d758d8fd111c9215a6e46abc704b6a0be838badd5` |
| PeerRegistry | `0x50a9982f6a3d6c1e6674f0fb4fa8b985007dbe19fc797abc691579be1f6493df` |
| ValidatorRegistry | `0x438d0ce63d40210d4e621cca6aaaf5d5438adaa54dfd71383fe41a82692a2561` |
| EnclaveRegistry | `0x442b82e471c1ee4577ea1f2168deb1f0b04fcc861ab79edb4b9c7d7738bf7f9f` |
| ScoreLedger | `0x0d2594727abeb45a13763baf8801ae765fbe41d147b28916ca78a0d08f73223a` |
| RoundState | `0xc1f18dc92629907641bc3176449af39738d2d8a93b4ad6b22548f4aed91d2611` |
| Hparams | `0x18b884530033f9b3e449b898c540ee5d3a25c4cab0abcf4843ef8e86e12adbfc` |
| RewardPool | `0x576ebeb78449ad46ef70dc3c5ca4e38d178846610bd7cf9f0764ae2f1dc0fe93` |

## Example `.env` — Miner

```bash
# Required
VRAMHUB_WALLET_MNEMONIC=your twelve or twenty four word mnemonic here

# Contract IDs (pre-filled for v0.7 testnet)
VRAMHUB_PACKAGE_ID=0xaff18bf6286047126901610d758d8fd111c9215a6e46abc704b6a0be838badd5
VRAMHUB_PEER_REGISTRY_ID=0x50a9982f6a3d6c1e6674f0fb4fa8b985007dbe19fc797abc691579be1f6493df
VRAMHUB_VALIDATOR_REGISTRY_ID=0x438d0ce63d40210d4e621cca6aaaf5d5438adaa54dfd71383fe41a82692a2561
VRAMHUB_REWARD_POOL_ID=0x576ebeb78449ad46ef70dc3c5ca4e38d178846610bd7cf9f0764ae2f1dc0fe93

# Storage: Walrus (free on testnet)
VRAMHUB_STORAGE_BACKEND=walrus
VRAMHUB_DEMO_MODE=true

# Optional: GPU device (auto-detected if not set)
# VRAMHUB_DEVICE=cuda:0
```

## Example `.env` — Validator (test mode)

```bash
# Required
VRAMHUB_WALLET_MNEMONIC=your twelve or twenty four word mnemonic here

# Contract IDs (pre-filled for v0.7 testnet)
VRAMHUB_PACKAGE_ID=0xaff18bf6286047126901610d758d8fd111c9215a6e46abc704b6a0be838badd5
VRAMHUB_PEER_REGISTRY_ID=0x50a9982f6a3d6c1e6674f0fb4fa8b985007dbe19fc797abc691579be1f6493df
VRAMHUB_VALIDATOR_REGISTRY_ID=0x438d0ce63d40210d4e621cca6aaaf5d5438adaa54dfd71383fe41a82692a2561
VRAMHUB_SCORE_LEDGER_ID=0x0d2594727abeb45a13763baf8801ae765fbe41d147b28916ca78a0d08f73223a
VRAMHUB_ROUND_STATE_ID=0xc1f18dc92629907641bc3176449af39738d2d8a93b4ad6b22548f4aed91d2611
VRAMHUB_HPARAMS_ID=0x18b884530033f9b3e449b898c540ee5d3a25c4cab0abcf4843ef8e86e12adbfc
VRAMHUB_REWARD_POOL_ID=0x576ebeb78449ad46ef70dc3c5ca4e38d178846610bd7cf9f0764ae2f1dc0fe93

# Validator UID (set after running: vram-cli register-validator)
VRAMHUB_VALIDATOR_UID=1
SLCL_VALIDATOR_UID=1

# Mode flags — both prefixes required
VRAMHUB_TEST_MODE=true
SLCL_TEST_MODE=true
VRAMHUB_NITRO_ENCLAVE=false
SLCL_NITRO_ENCLAVE=false
SLCL_SKIP_SEAL=true
```
