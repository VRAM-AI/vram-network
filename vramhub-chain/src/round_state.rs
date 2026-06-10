// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

//! RoundState on-chain queries.

use sui_sdk::SuiClient;
use sui_types::base_types::ObjectID;

use vramhub_core::VramhubError;

pub async fn get_checkpoint_hash(
    sui_client: &SuiClient,
    round_state_id: ObjectID,
    window: u64,
) -> Result<Option<[u8; 32]>, VramhubError> {
    let window_record = fetch_window_record(sui_client, round_state_id, window).await?;
    let record = match window_record {
        Some(r) => r,
        None => return Ok(None),
    };

    let hash_arr = &record["checkpoint_hash"];
    if hash_arr.is_null() {
        return Ok(None);
    }

    let bytes: Vec<u8> = serde_json::from_value(hash_arr.clone())
        .map_err(|e| VramhubError::SerializationError(e.to_string()))?;

    if bytes.len() != 32 {
        return Err(VramhubError::SerializationError(format!(
            "checkpoint_hash expected 32 bytes, got {}",
            bytes.len()
        )));
    }

    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(Some(arr))
}

pub async fn get_top_g_peers(
    sui_client: &SuiClient,
    round_state_id: ObjectID,
    window: u64,
) -> Result<Vec<u64>, VramhubError> {
    let window_record = fetch_window_record(sui_client, round_state_id, window).await?;
    let record = match window_record {
        Some(r) => r,
        None => return Ok(vec![]),
    };

    let peers = record["top_g_peers"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    v.as_u64()
                        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(peers)
}

/// Fetch the window record from RoundState.windows dynamic field.
async fn fetch_window_record(
    sui_client: &SuiClient,
    round_state_id: ObjectID,
    window: u64,
) -> Result<Option<serde_json::Value>, VramhubError> {
    use sui_types::dynamic_field::DynamicFieldName;

    // The RoundState.windows table uses window number (u64) as the key
    let _key = serde_json::json!({
        "type": "u64",
        "value": window.to_string()
    });

    let field_name = DynamicFieldName {
        type_: "u64"
            .parse()
            .map_err(|e: _| VramhubError::Internal(format!("{e}")))?,
        value: serde_json::Value::String(window.to_string()),
    };

    let resp = match sui_client
        .read_api()
        .get_dynamic_field_object(round_state_id, field_name)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("not found") || msg.contains("404") {
                return Ok(None);
            }
            return Err(VramhubError::RpcError(msg));
        }
    };

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

    // Dynamic field layout: { name: window, value: WindowRecord }
    Ok(Some(json["fields"]["value"]["fields"].clone()))
}
