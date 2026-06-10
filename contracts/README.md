# VRAM HUB — Move Contracts

Sui Move smart contracts for the VRAM decentralized LLM training coordination protocol.

## Testnet Deployment (2026-03-30, v2 — VRAM token)

| Object | ID |
|--------|-----|
| **Package** | `0xaff18bf6286047126901610d758d8fd111c9215a6e46abc704b6a0be838badd5` |
| PeerRegistry | `0x50a9982f6a3d6c1e6674f0fb4fa8b985007dbe19fc797abc691579be1f6493df` |
| ValidatorRegistry | `0x438d0ce63d40210d4e621cca6aaaf5d5438adaa54dfd71383fe41a82692a2561` |
| EnclaveRegistry | `0x442b82e471c1ee4577ea1f2168deb1f0b04fcc861ab79edb4b9c7d7738bf7f9f` |
| ScoreLedger | `0x0d2594727abeb45a13763baf8801ae765fbe41d147b28916ca78a0d08f73223a` |
| RoundState | `0xc1f18dc92629907641bc3176449af39738d2d8a93b4ad6b22548f4aed91d2611` |
| Hparams | `0x18b884530033f9b3e449b898c540ee5d3a25c4cab0abcf4843ef8e86e12adbfc` |
| RewardPool | `0x576ebeb78449ad46ef70dc3c5ca4e38d178846610bd7cf9f0764ae2f1dc0fe93` |

**Deployer:** `0xb7aaeb31d576814e1b268a43033feccac19a2905a652ad3b42fb5efeb1c772c3`
**Network:** Sui Testnet (`4c78adac`)
**Token:** VRAM · symbol `VRAM` · 9 decimals · 500M hard cap
**Explorer:** https://suiscan.xyz/testnet/object/0xaff18bf6286047126901610d758d8fd111c9215a6e46abc704b6a0be838badd5

### RewardPool Status

| Parameter | Value |
|-----------|-------|
| Balance | 6,000,000 VRAM |
| Emission/window | 1,200 VRAM |
| Runway | ~5,000 windows (~34 days) |
| Halving schedule | Every 4 years (governance via `hparams::update_emission`) |

## Seal IBE Key Servers (Testnet)

| Server | Object ID |
|--------|-----------|
| Mysten KS1 | `0x73d05d62c18d9374e3ea529e8e0ed6161da1a141a94d3f76ae3fe4e99356db75` |
| Mysten KS2 | `0xf5d14a81a982144ae441cd7d64b09027f116a468bd36e7eca494f750591623c8` |

Threshold: **2-of-2**. Both servers must respond for credential decryption to succeed.

## Modules

| Module | Description |
|--------|-------------|
| `peer_registry.move` | Miner/validator registration; stores Seal IBE-encrypted R2 credentials |
| `validator_registry.move` | Validator stake management; `seal_approve` access control gate |
| `enclave_registry.move` | Nitro enclave PCR registration and Ed25519 public key storage |
| `score_ledger.move` | Verifies enclave Ed25519 signatures; stores per-window OpenSkill scores |
| `round_state.move` | Window clock; anchors SHA-256 checkpoint hashes on-chain |
| `hparams.move` | On-chain hyperparameters (governance-updatable) |
| `reward_distributor.move` | Per-window VRAM token emission proportional to OpenSkill weights |
| `seal_policy.move` | `seal_approve` entry function — called by Seal key servers to verify validators |
| `vram_token.move` | VRAM reward token (OTW pattern, `TreasuryCap` held by deployer) |

## Building & Testing

```bash
cd contracts

# Run all unit tests
sui move test

# Expected output:
# [ PASS ] slcl::seal_policy_tests::test_seal_approve_registered_validator
# [ PASS ] slcl::enclave_registry_tests::test_deactivate_enclave_wrong_owner
# [ PASS ] slcl::enclave_registry_tests::test_is_registered_false_for_unknown_uid
# [ PASS ] slcl::seal_policy_tests::test_seal_approve_unregistered_fails
# [ PASS ] slcl::enclave_registry_tests::test_register_enclave
# [ PASS ] slcl::enclave_registry_tests::test_register_enclave_pcr_mismatch
# [ PASS ] slcl::enclave_registry_tests::test_update_expected_pcrs
# Test result: OK. Total tests: 7; passed: 7; failed: 0
```

## Deploying

```bash
# Switch to testnet
sui client switch --env testnet
# Faucet: https://faucet.sui.io  (CLI faucet not available on testnet)

# Deploy (clear Published.toml testnet entry first if redeploying)
cd contracts
sui client publish --skip-dependency-verification
```

After deployment, copy the published object IDs into `.env` and `vramscan/lib/api-real.ts`.

Post-deploy setup (run with deployer wallet):
```bash
PKG=<new-package-id>

# 1. Set emission to 1,200 VRAM/window
sui client call --package $PKG --module hparams --function update_emission \
  --args <hparams-id> <hparams-admin-cap> 1200000000000 --gas-budget 10000000

# 2. Create reward pool
sui client call --package $PKG --module reward_distributor --function create_pool \
  --type-args ${PKG}::vram_token::VRAM_TOKEN \
  --args <distributor-admin-cap> 1200000000000 --gas-budget 50000000

# 3. Mint 6M VRAM and deposit
sui client ptb \
  --move-call "${PKG}::vram_token::mint" @<treasury-cap> 6000000000000000 \
  --assign vram_coin \
  --move-call "${PKG}::reward_distributor::deposit<${PKG}::vram_token::VRAM_TOKEN>" @<pool-id> vram_coin \
  --gas-budget 50000000
```

## Previous Deployments

| Date | Network | Token | Package ID |
|------|---------|-------|------------|
| 2026-03-27 | devnet | TPLR | `0x6f54291adacc2cb407725d641e5a9493f1309195364d307f93216193accf4911` |
| 2026-03-30 | devnet | TPLR | `0x794814c27e91bb927613253331b17b82906d4beaa1e5329f027dc8765c06359a` |
| 2026-03-30 | testnet | TPLR | `0x5442bc20ef2989b9db57b62747e64c6b1050cc1812bbf15ebaa294fc4fe84fcf` |
| 2026-03-30 | **testnet** | **VRAM** | `0xaff18bf6286047126901610d758d8fd111c9215a6e46abc704b6a0be838badd5` ← **current** |
