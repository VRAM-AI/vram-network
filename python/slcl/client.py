"""
SLCL Python client - thin wrapper around the Rust SuiChainClient via PyO3.

Usage:
    from slcl.client import SlclClient

    client = SlclClient.from_env()

    # These call directly into Rust
    window = await client.current_window()
    peers = await client.fetch_peers()
    await client.register_peer(credentials, stake, peer_type)
    await client.submit_scores(window, scores, enclave_signature)
"""

from slcl._native import (
    SlclClientNative,
    PeerType as _PeerType,
)


class PeerType:
    MINER = _PeerType.Miner
    VALIDATOR = _PeerType.Validator


class SlclClient:
    """
    Python interface to the SLCL chain client.

    All methods delegate to Rust via PyO3. No chain logic lives in Python.
    """

    def __init__(self, native: SlclClientNative):
        self._native = native

    @classmethod
    def from_env(cls) -> "SlclClient":
        """Initialize from environment variables (same as Rust ChainConfig::from_env)."""
        return cls(SlclClientNative.from_env())

    async def current_window(self) -> int:
        return await self._native.current_window()

    async def register_peer(
        self,
        read_access_key: str,
        read_secret_key: str,
        bucket_name: str,
        account_id: str,
        stake_amount: int,
        peer_type: int = PeerType.MINER,
    ) -> int:
        """Register as a peer. Returns assigned UID."""
        return await self._native.register_peer(
            read_access_key, read_secret_key,
            bucket_name, account_id,
            stake_amount, peer_type,
        )

    async def fetch_peers(self) -> list:
        """Fetch all registered peers. Validators get decrypted credentials."""
        return await self._native.fetch_peers()

    async def submit_scores(
        self,
        window: int,
        scores: dict,
        enclave_signature: bytes,
        checkpoint_hash: bytes,
        enclave_object_id: str,
    ) -> None:
        """Submit TEE-signed scores on-chain."""
        await self._native.submit_scores(
            window, scores, enclave_signature,
            checkpoint_hash, enclave_object_id,
        )

    async def get_hparams(self) -> dict:
        return await self._native.get_hparams()
