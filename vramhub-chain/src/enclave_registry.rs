// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

//! EnclaveRegistry on-chain queries.

use sui_sdk::SuiClient;
use sui_types::base_types::ObjectID;
use sui_types::dynamic_field::DynamicFieldName;

use vramhub_core::{EnclaveInfo, VramhubError};

pub async fn get_enclave_info(
    sui_client: &SuiClient,
    registry_id: ObjectID,
    validator_uid: u64,
) -> Result<Option<EnclaveInfo>, VramhubError> {
    let field_name = DynamicFieldName {
        type_: "u64"
            .parse()
            .map_err(|e: _| VramhubError::Internal(format!("{e}")))?,
        value: serde_json::Value::String(validator_uid.to_string()),
    };

    let resp = match sui_client
        .read_api()
        .get_dynamic_field_object(registry_id, field_name)
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

    let obj_id = data.object_id.to_string();
    let content = match data.content {
        Some(c) => c,
        None => return Ok(None),
    };

    let json = serde_json::to_value(content)
        .map_err(|e| VramhubError::SerializationError(e.to_string()))?;

    // Dynamic field layout: { name: uid, value: EnclaveRecord }
    let record = &json["fields"]["value"]["fields"];
    parse_enclave_info(record, obj_id)
}

pub async fn is_enclave_registered(
    sui_client: &SuiClient,
    registry_id: ObjectID,
    validator_uid: u64,
) -> Result<bool, VramhubError> {
    let info = get_enclave_info(sui_client, registry_id, validator_uid).await?;
    Ok(info.map(|i| i.is_active).unwrap_or(false))
}

fn parse_enclave_info(
    fields: &serde_json::Value,
    object_id: String,
) -> Result<Option<EnclaveInfo>, VramhubError> {
    fn parse_bytes(v: &serde_json::Value) -> Vec<u8> {
        v.as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|b| b.as_u64().map(|n| n as u8))
                    .collect()
            })
            .unwrap_or_default()
    }
    let validator_uid = fields["validator_uid"]
        .as_u64()
        .or_else(|| {
            fields["validator_uid"]
                .as_str()
                .and_then(|s| s.parse().ok())
        })
        .ok_or_else(|| VramhubError::SerializationError("missing validator_uid".to_string()))?;

    Ok(Some(EnclaveInfo {
        object_id,
        enclave_pubkey: parse_bytes(&fields["enclave_pubkey"]),
        pcr0: parse_bytes(&fields["pcr0"]),
        pcr1: parse_bytes(&fields["pcr1"]),
        pcr2: parse_bytes(&fields["pcr2"]),
        validator_uid,
        is_active: fields["is_active"].as_bool().unwrap_or(false),
    }))
}
