// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

//! ScoreLedger on-chain queries.

use sui_sdk::SuiClient;
use sui_types::base_types::ObjectID;
use sui_types::dynamic_field::DynamicFieldName;

use vramhub_core::{PeerScore, VramhubError};

pub async fn get_peer_scores(
    sui_client: &SuiClient,
    ledger_id: ObjectID,
    uids: &[u64],
) -> Result<Vec<PeerScore>, VramhubError> {
    let mut scores = Vec::with_capacity(uids.len());

    for &uid in uids {
        match fetch_score_record(sui_client, ledger_id, uid).await {
            Ok(Some(score)) => scores.push(score),
            Ok(None) => {} // Peer has no score record yet
            Err(e) => tracing::warn!("failed to fetch score for uid {uid}: {e}"),
        }
    }

    Ok(scores)
}

async fn fetch_score_record(
    sui_client: &SuiClient,
    ledger_id: ObjectID,
    uid: u64,
) -> Result<Option<PeerScore>, VramhubError> {
    let field_name = DynamicFieldName {
        type_: "u64"
            .parse()
            .map_err(|e: _| VramhubError::Internal(format!("{e}")))?,
        value: serde_json::Value::String(uid.to_string()),
    };

    let resp = match sui_client
        .read_api()
        .get_dynamic_field_object(ledger_id, field_name)
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

    // Dynamic field layout: { name: uid, value: ScoreRecord }
    let record = &json["fields"]["value"]["fields"];
    parse_peer_score(record, uid)
}

fn parse_peer_score(
    fields: &serde_json::Value,
    uid: u64,
) -> Result<Option<PeerScore>, VramhubError> {
    fn get_u64(v: &serde_json::Value, f: &str) -> u64 {
        v[f].as_u64()
            .or_else(|| v[f].as_str().and_then(|s| s.parse().ok()))
            .unwrap_or(0)
    }
    fn get_i64(v: &serde_json::Value, f: &str) -> i64 {
        v[f].as_i64()
            .or_else(|| v[f].as_str().and_then(|s| s.parse().ok()))
            .unwrap_or(0)
    }

    Ok(Some(PeerScore {
        uid,
        openskill_mu: get_u64(fields, "openskill_mu"),
        openskill_sigma: get_u64(fields, "openskill_sigma"),
        mu_generalization: get_i64(fields, "mu_generalization"),
        peer_score: get_u64(fields, "peer_score"),
        normalized_weight: get_u64(fields, "normalized_weight"),
        last_updated_window: get_u64(fields, "last_updated_window"),
    }))
}
