# peer\_registry.move

Manages peer (miner and validator) registration and stores IBE-encrypted R2 credentials on-chain.

## Shared Object

`PeerRegistry` — created once at package publish; shared with all nodes.

## Key Types

```move
public struct PeerRegistry has key {
    id: UID,
    peers: Table<u64, PeerInfo>,
    next_uid: u64,
    admin: address,
}

public struct PeerInfo has store {
    uid: u64,
    owner: address,
    role: u8,              // 0 = miner, 1 = validator
    stake: u64,
    registered_at_ms: u64,
    r2_bucket: String,
    r2_account_id: String,
    /// IBE-encrypted R2 read credentials (Seal ciphertext)
    encrypted_r2_creds: vector<u8>,
    active: bool,
}
```

## Entry Functions

### `register_peer`

```move
public entry fun register_peer(
    registry: &mut PeerRegistry,
    role: u8,
    stake: u64,
    r2_bucket: vector<u8>,
    r2_account_id: vector<u8>,
    encrypted_r2_creds: vector<u8>,
    clock: &sui::clock::Clock,
    ctx: &mut TxContext,
)
```

Registers a new peer and assigns a UID. The `encrypted_r2_creds` field contains the Seal IBE ciphertext — only staked validators can decrypt it via `seal_policy.move`.

### `update_credentials`

```move
public entry fun update_credentials(
    registry: &mut PeerRegistry,
    uid: u64,
    encrypted_r2_creds: vector<u8>,
    ctx: &mut TxContext,
)
```

Allows a registered peer to rotate their R2 credentials. Only callable by the peer's registered `owner` address.

### `deactivate_peer`

```move
public entry fun deactivate_peer(
    registry: &mut PeerRegistry,
    uid: u64,
    ctx: &mut TxContext,
)
```

Marks a peer as inactive. Inactive peers are not evaluated in future windows.

## View Functions

```move
public fun get_peer(registry: &PeerRegistry, uid: u64): &PeerInfo
public fun is_active(registry: &PeerRegistry, uid: u64): bool
public fun peer_count(registry: &PeerRegistry): u64
public fun get_encrypted_creds(registry: &PeerRegistry, uid: u64): &vector<u8>
```

## Error Codes

| Code | Constant | Meaning |
|------|----------|---------|
| 1 | `E_PEER_NOT_FOUND` | UID not in registry |
| 2 | `E_NOT_OWNER` | Caller is not the peer's registered owner |
| 3 | `E_ALREADY_REGISTERED` | Address already has a registered peer |
| 4 | `E_INSUFFICIENT_STAKE` | Stake below minimum required |

## Credential Privacy

The `encrypted_r2_creds` field is the output of Seal IBE encryption. The plaintext is:

```
r2_access_key_id || "|" || r2_secret_access_key
```

The IBE identity is the validator set — any staked, active validator can reconstruct the IBE key via Seal key servers (subject to `seal_approve` in `seal_policy.move`).
