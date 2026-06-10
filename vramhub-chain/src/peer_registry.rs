// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

//! PeerRegistry on-chain queries.
//!
//! The on-chain layout is:
//!
//!   PeerRegistry (shared object)
//!   ├── peers: Table<u64, PeerRecord>      ← dynamic-field child object
//!   │   ├── <uid=0>: PeerRecord
//!   │   └── <uid=1>: PeerRecord
//!   └── address_to_uid: Table<address, u64> ← dynamic-field child object
//!
//! To list all peers we must:
//!   1. Fetch the PeerRegistry object to find the `peers` Table's object ID.
//!   2. Call get_dynamic_fields on the Table object to enumerate PeerRecords.

use sui_sdk::rpc_types::SuiObjectDataOptions;
use sui_sdk::SuiClient;
use sui_types::base_types::ObjectID;

use vramhub_core::{PeerInfo, PeerType, VramhubError};

// ── Public API ────────────────────────────────────────────────────────────────

pub async fn fetch_all_peers(
    sui_client: &SuiClient,
    registry_id: ObjectID,
) -> Result<Vec<PeerInfo>, VramhubError> {
    let peers_table_id = get_peers_table_id(sui_client, registry_id).await?;
    fetch_peers_from_table(sui_client, peers_table_id).await
}

pub async fn get_peer_by_address(
    sui_client: &SuiClient,
    registry_id: ObjectID,
    address: &str,
) -> Result<Option<PeerInfo>, VramhubError> {
    let peers = fetch_all_peers(sui_client, registry_id).await?;
    Ok(peers.into_iter().find(|p| p.address == address))
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Get the object ID of the `peers: Table<u64, PeerRecord>` inside the PeerRegistry.
async fn get_peers_table_id(
    sui_client: &SuiClient,
    registry_id: ObjectID,
) -> Result<ObjectID, VramhubError> {
    let resp = sui_client
        .read_api()
        .get_object_with_options(registry_id, SuiObjectDataOptions::new().with_content())
        .await
        .map_err(|e| VramhubError::RpcError(e.to_string()))?;

    let data = resp
        .data
        .ok_or_else(|| VramhubError::RpcError("PeerRegistry object not found".to_string()))?;

    let content = data
        .content
        .ok_or_else(|| VramhubError::RpcError("PeerRegistry has no content".to_string()))?;

    let json = serde_json::to_value(content)
        .map_err(|e| VramhubError::SerializationError(e.to_string()))?;

    // Content layout:
    //   { type: "...", fields: { peers: { type: "0x2::table::Table<...>", fields: { id: { id: "<object-id>" }, ... } }, ... } }
    let peers_id_str = json["fields"]["peers"]["fields"]["id"]["id"]
        .as_str()
        .ok_or_else(|| {
            VramhubError::SerializationError(format!(
                "cannot extract peers table id from registry content: {json}"
            ))
        })?;

    peers_id_str.parse::<ObjectID>().map_err(|e| {
        VramhubError::SerializationError(format!("invalid peers table id {peers_id_str}: {e}"))
    })
}

/// Enumerate all PeerRecord dynamic fields from the `peers` Table.
async fn fetch_peers_from_table(
    sui_client: &SuiClient,
    table_id: ObjectID,
) -> Result<Vec<PeerInfo>, VramhubError> {
    let mut peers = Vec::new();
    let mut cursor = None;

    loop {
        let page = sui_client
            .read_api()
            .get_dynamic_fields(table_id, cursor, Some(50))
            .await
            .map_err(|e| VramhubError::RpcError(e.to_string()))?;

        for field_info in &page.data {
            match fetch_peer_record(sui_client, field_info.object_id).await {
                Ok(Some(peer)) => peers.push(peer),
                Ok(None) => {}
                Err(e) => tracing::warn!("skipping peer record {}: {e}", field_info.object_id),
            }
        }

        if !page.has_next_page {
            break;
        }
        cursor = page.next_cursor;
    }

    Ok(peers)
}

async fn fetch_peer_record(
    sui_client: &SuiClient,
    field_object_id: ObjectID,
) -> Result<Option<PeerInfo>, VramhubError> {
    let resp = sui_client
        .read_api()
        .get_object_with_options(field_object_id, SuiObjectDataOptions::new().with_content())
        .await
        .map_err(|e| VramhubError::RpcError(e.to_string()))?;

    let data = match resp.data {
        Some(d) => d,
        None => return Ok(None),
    };

    let content = match data.content {
        Some(c) => c,
        None => return Ok(None),
    };

    let json = serde_json::to_value(content)
        .map_err(|e| VramhubError::SerializationError(e.to_string()))?;

    // Dynamic field layout: { fields: { name: <uid>, value: { fields: <PeerRecord> } } }
    let record = &json["fields"]["value"]["fields"];
    parse_peer_info(record)
}

fn parse_peer_info(fields: &serde_json::Value) -> Result<Option<PeerInfo>, VramhubError> {
    use vramhub_core::EncryptedBucket;

    let uid = fields["uid"]
        .as_u64()
        .or_else(|| fields["uid"].as_str().and_then(|s| s.parse().ok()))
        .ok_or_else(|| VramhubError::SerializationError("missing peer uid".to_string()))?;

    let address = fields["owner"].as_str().unwrap_or_default().to_string();

    let stake = fields["stake"]
        .as_u64()
        .or_else(|| fields["stake"].as_str().and_then(|s| s.parse().ok()))
        .unwrap_or(0);

    let registered_at_window = fields["registered_at_window"].as_u64().unwrap_or(0);

    let peer_type_u8 = fields["peer_type"].as_u64().unwrap_or(0) as u8;
    let peer_type = if peer_type_u8 == 0 {
        PeerType::Miner
    } else {
        PeerType::Validator
    };

    let is_active = fields["is_active"].as_bool().unwrap_or(true);

    let eb = &fields["encrypted_bucket"]["fields"];
    let encrypted_bucket = EncryptedBucket {
        name: eb["name"].as_str().unwrap_or_default().to_string(),
        account_id: eb["account_id"].as_str().unwrap_or_default().to_string(),
        endpoint: eb["endpoint"].as_str().map(|s| s.to_string()),
        seal_encrypted_object: parse_byte_vec(&eb["seal_encrypted_object"]),
        seal_identity: parse_byte_vec(&eb["seal_identity"]),
        seal_package_id: eb["seal_package_id"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        key_server_object_ids: eb["key_server_object_ids"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
        threshold: eb["threshold"].as_u64().unwrap_or(2) as u8,
    };

    Ok(Some(PeerInfo {
        uid,
        address,
        encrypted_bucket,
        stake,
        registered_at_window,
        peer_type,
        is_active,
    }))
}

fn parse_byte_vec(json: &serde_json::Value) -> Vec<u8> {
    json.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_u64().map(|b| b as u8))
                .collect()
        })
        .unwrap_or_default()
}
