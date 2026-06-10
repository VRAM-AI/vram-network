// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

//! ValidatorRegistry on-chain queries.

use sui_sdk::SuiClient;
use sui_types::base_types::ObjectID;
use sui_types::dynamic_field::DynamicFieldName;

use vramhub_core::VramhubError;

pub async fn is_registered_validator(
    sui_client: &SuiClient,
    registry_id: ObjectID,
    address: &str,
) -> Result<bool, VramhubError> {
    let stake = get_validator_stake(sui_client, registry_id, address).await;
    Ok(stake.is_ok())
}

pub async fn get_validator_stake(
    sui_client: &SuiClient,
    registry_id: ObjectID,
    address: &str,
) -> Result<u64, VramhubError> {
    // The ValidatorRegistry.validators table is keyed by address (string)
    let field_name = DynamicFieldName {
        type_: "address"
            .parse()
            .map_err(|e: _| VramhubError::Internal(format!("{e}")))?,
        value: serde_json::Value::String(address.to_string()),
    };

    let resp = sui_client
        .read_api()
        .get_dynamic_field_object(registry_id, field_name)
        .await
        .map_err(|e| VramhubError::RpcError(e.to_string()))?;

    let data = resp.data.ok_or_else(|| VramhubError::PeerNotRegistered {
        address: address.to_string(),
    })?;

    let content = data
        .content
        .ok_or_else(|| VramhubError::RpcError("validator record has no content".to_string()))?;

    let json = serde_json::to_value(content)
        .map_err(|e| VramhubError::SerializationError(e.to_string()))?;

    let record = &json["fields"]["value"]["fields"];
    let stake = record["stake"]
        .as_u64()
        .or_else(|| record["stake"].as_str().and_then(|s| s.parse().ok()))
        .ok_or_else(|| VramhubError::SerializationError("missing stake field".to_string()))?;

    Ok(stake)
}
