# Environment Variables

All VRAM HUB binaries are configured via environment variables. Copy `.env.example` to `.env` and fill in the required values.

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

## Miner-Only

| Variable | Description | Required |
|----------|-------------|----------|
| `VRAMHUB_MINER_UID` | Your registered peer UID. **Optional** — auto-registered on first startup and saved to `.vramhub-uid` | No |
| `VRAMHUB_R2_ACCOUNT_ID` | Cloudflare account ID | Yes |
| `VRAMHUB_R2_BUCKET_NAME` | R2 bucket name for gradient uploads | Yes |
| `VRAMHUB_R2_ACCESS_KEY_ID` | R2 access key with read+write permissions | Yes |
| `VRAMHUB_R2_SECRET_ACCESS_KEY` | R2 secret access key | Yes |
| `VRAMHUB_BATCH_SIZE` | Training batch size (default: `4` for nano-GPT) | No |
| `VRAMHUB_DEVICE` | Training device override: `cpu`, `cuda`, `cuda:0`, `cuda:1`, `metal` (default: auto-detect) | No |
| `VRAMHUB_SIDECAR_URL` | Python sidecar URL when using `--features sidecar` (default: `http://127.0.0.1:17070`). Windows reserves 7009-7108, so the historical default of 7070 was changed to 17070. | No |
| `VRAMHUB_SKIP_SEAL` | Set to `true` to skip Seal IBE credential encryption during testing | No |

## Testnet / Points Tracker

| Variable | Description | Required |
|----------|-------------|----------|
| `VRAMHUB_TESTNET_MODE` | Set to `true` to enable testnet-specific behaviour (points accrual, genesis miner tracking). Automatically set when running against testnet RPC. | No |
| `VRAMHUB_POINTS_API_URL` | URL of the `vramhub-points` REST API (default: `http://localhost:8080`). VRAMScan reads this for the leaderboard and genesis-miners pages. | No |
| `VRAMHUB_EARLY_BONUS_END` | Unix timestamp (ms) when the 2× early-miner points bonus expires. Set to `VRAMHUB_POINTS_GENESIS_MS + 7776000000` (90 days). If unset, early bonus is always active. | No |
| `VRAMHUB_MINER_BPS` | Miner share of per-window emission in basis points (default: `10000` on testnet, `7200` post-v0.5). | No |
| `VRAMHUB_VALIDATOR_BPS` | Validator share of per-window emission in basis points (default: `0` on testnet, `1800` post-v0.5). | No |
| `VRAMHUB_TREASURY_BPS` | Treasury share of per-window emission in basis points (default: `0` on testnet, `1000` post-v0.5). `MINER_BPS + VALIDATOR_BPS + TREASURY_BPS` must equal `10000`. | No |

## Validator-Only

| Variable | Description | Required |
|----------|-------------|----------|
| `VRAMHUB_VALIDATOR_UID` | Your registered validator UID | Yes |
| `VRAMHUB_NAUTILUS_URL` | Enclave HTTP endpoint (default: `http://localhost:3000`) | Yes |
| `VRAMHUB_SEAL_KEY_SERVER_IDS` | Comma-separated Seal key server object IDs | Yes |
| `VRAMHUB_SEAL_THRESHOLD` | Seal key server threshold t-of-n (default: `2`) | No |
| `VRAMHUB_R2_ACCOUNT_ID` | Cloudflare account ID (for downloading miner gradients) | Yes |
| `VRAMHUB_R2_ACCESS_KEY_ID` | R2 access key | Yes |
| `VRAMHUB_R2_SECRET_ACCESS_KEY` | R2 secret access key | Yes |

## Testnet Object IDs

All values are pre-filled in `.env.example`. Only `VRAMHUB_WALLET_MNEMONIC` and R2 credentials need to be set.

| Object | ID |
|--------|----|
| Package | `0xaff18bf6286047126901610d758d8fd111c9215a6e46abc704b6a0be838badd5` |
| PeerRegistry | `0x50a9982f6a3d6c1e6674f0fb4fa8b985007dbe19fc797abc691579be1f6493df` |
| ValidatorRegistry | `0x438d0ce63d40210d4e621cca6aaaf5d5438adaa54dfd71383fe41a82692a2561` |
| RewardPool | `0x576ebeb78449ad46ef70dc3c5ca4e38d178846610bd7cf9f0764ae2f1dc0fe93` |

## Example `.env`

```bash
# Sui — testnet
VRAMHUB_SUI_RPC_URL=https://fullnode.testnet.sui.io:443
VRAMHUB_WALLET_MNEMONIC=your twelve or twenty four word mnemonic here
VRAMHUB_PACKAGE_ID=0xaff18bf6286047126901610d758d8fd111c9215a6e46abc704b6a0be838badd5

# Shared objects (pre-filled for testnet)
VRAMHUB_PEER_REGISTRY_ID=0x50a9982f6a3d6c1e6674f0fb4fa8b985007dbe19fc797abc691579be1f6493df
VRAMHUB_VALIDATOR_REGISTRY_ID=0x438d0ce63d40210d4e621cca6aaaf5d5438adaa54dfd71383fe41a82692a2561
VRAMHUB_REWARD_POOL_ID=0x576ebeb78449ad46ef70dc3c5ca4e38d178846610bd7cf9f0764ae2f1dc0fe93

# Miner
VRAMHUB_MINER_UID=            # leave blank — auto-registered on first run
VRAMHUB_R2_ACCOUNT_ID=your_cloudflare_account_id
VRAMHUB_R2_BUCKET_NAME=vram-gradients-yourname
VRAMHUB_R2_ACCESS_KEY_ID=your_r2_access_key
VRAMHUB_R2_SECRET_ACCESS_KEY=your_r2_secret_key

# Optional: GPU device (auto-detected if not set)
# VRAMHUB_DEVICE=cuda:0

# Optional: Python sidecar URL (only needed with --features sidecar)
# VRAMHUB_SIDECAR_URL=http://127.0.0.1:17070

# Testnet points tracker (vramhub-points binary)
VRAMHUB_TESTNET_MODE=true
VRAMHUB_POINTS_API_URL=http://localhost:8080
# VRAMHUB_EARLY_BONUS_END=    # set to genesis_ms + 7776000000 (90 days)
# VRAMHUB_MINER_BPS=10000     # testnet: 100% to miners; mainnet v0.5: 7200
# VRAMHUB_VALIDATOR_BPS=0     # mainnet v0.5: 1800
# VRAMHUB_TREASURY_BPS=0      # mainnet v0.5: 1000

# Validator
VRAMHUB_VALIDATOR_UID=
VRAMHUB_NAUTILUS_URL=http://localhost:3000
VRAMHUB_SEAL_THRESHOLD=2
```
