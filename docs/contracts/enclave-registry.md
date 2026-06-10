# enclave\_registry.move

Stores Nitro enclave registrations: PCR values and Ed25519 public keys. This is the source of truth for enclave identity — `score_ledger.move` reads enclave public keys from here to verify score signatures.

## Shared Object

`EnclaveRegistry` — created at package publish; shared with all nodes.

## Key Types

```move
public struct EnclaveRegistry has key {
    id: UID,
    enclaves: Table<u64, EnclaveInfo>,
    admin: address,
}

public struct EnclaveInfo has store {
    validator_uid: u64,
    enclave_pubkey: vector<u8>,   // Ed25519 public key (32 bytes)
    pcr0: vector<u8>,             // SHA-384, 48 bytes
    pcr1: vector<u8>,             // SHA-384, 48 bytes
    pcr2: vector<u8>,             // SHA-384, 48 bytes
    registered_at_ms: u64,
    attestation_doc: vector<u8>,  // Full COSE_Sign1 document (stored for auditability)
}
```

## Entry Functions

### `register_enclave`

```move
public entry fun register_enclave(
    registry: &mut EnclaveRegistry,
    hparams: &slcl::hparams::Hparams,
    validator_uid: u64,
    attestation_document: vector<u8>,
    enclave_pubkey: vector<u8>,
    pcr0: vector<u8>,
    pcr1: vector<u8>,
    pcr2: vector<u8>,
    clock: &sui::clock::Clock,
    ctx: &mut TxContext,
)
```

Verifies and records a Nitro enclave registration:

1. Verifies the COSE_Sign1 attestation document against the AWS Nitro root certificate
2. Extracts PCR values from the attestation payload
3. Checks that PCR0, PCR1, PCR2 match the expected values in `hparams.move`
4. Records the enclave public key

This is the **expensive** step. It runs once per enclave binary version. After registration, all score submissions use cheap Ed25519 signature verification.

### `update_enclave_key`

```move
public entry fun update_enclave_key(
    registry: &mut EnclaveRegistry,
    hparams: &slcl::hparams::Hparams,
    validator_uid: u64,
    attestation_document: vector<u8>,
    enclave_pubkey: vector<u8>,
    pcr0: vector<u8>,
    pcr1: vector<u8>,
    pcr2: vector<u8>,
    clock: &sui::clock::Clock,
    ctx: &mut TxContext,
)
```

Updates the enclave public key after an enclave restart (new ephemeral keypair). PCR values must still match — only the public key changes.

## View Functions Used by `score_ledger.move`

```move
public fun is_registered(registry: &EnclaveRegistry, validator_uid: u64): bool
public fun get_enclave_pubkey(registry: &EnclaveRegistry, validator_uid: u64): vector<u8>
```

## Error Codes

| Code | Constant | Meaning |
|------|----------|---------|
| 1 | `E_ENCLAVE_NOT_FOUND` | Validator UID has no registered enclave |
| 2 | `E_PCR_MISMATCH` | PCR values don't match `hparams.move` |
| 3 | `E_ATTESTATION_INVALID` | Nitro attestation document verification failed |
| 4 | `E_PUBKEY_INVALID` | Public key is not 32 bytes |

## PCR Values in Hparams

Expected PCR values are stored in `hparams.move` as:

```move
expected_pcr0: vector<u8>,  // 48 bytes
expected_pcr1: vector<u8>,  // 48 bytes
expected_pcr2: vector<u8>,  // 48 bytes
```

Updating the enclave binary requires a governance call to `hparams.move` to update the expected PCRs, followed by re-registering enclaves with the new binary.
