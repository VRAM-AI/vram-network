// SPDX-License-Identifier: BUSL-1.1
// Copyright (c) 2024-2025 VRAM AI Limited

//! `register-enclave` command.
//!
//! Fetches the attestation document from a running enclave and
//! submits it to enclave_registry::register_enclave on-chain.
//!
//! The attestation document is a COSE_Sign1 CBOR structure. This module
//! extracts PCR0/1/2 (48-byte SHA-384 measurements) from the payload.

use anyhow::Result;
use ciborium::value::Value;
use vramhub_chain::SuiChainClient;

pub async fn register_enclave(
    chain: &SuiChainClient,
    enclave_url: &str,
    validator_uid: u64,
) -> Result<()> {
    tracing::info!(enclave_url, validator_uid, "Fetching attestation document");

    // 1. Call GET /get_attestation on the enclave
    let http = reqwest::Client::new();
    let resp = http
        .get(format!("{enclave_url}/get_attestation"))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    let attestation_doc_hex = resp["attestation_document"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing attestation_document in response"))?;
    let attestation_document = hex::decode(attestation_doc_hex)?;

    let pubkey_hex = resp["public_key"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing public_key in response"))?;
    let enclave_pubkey = hex::decode(pubkey_hex)?;

    // 2. Extract PCR values from the CBOR attestation document.
    let (pcr0, pcr1, pcr2) = extract_pcrs(&attestation_document)?;

    tracing::info!(
        pubkey = pubkey_hex,
        pcr0 = %hex::encode(&pcr0),
        pcr1 = %hex::encode(&pcr1),
        pcr2 = %hex::encode(&pcr2),
        "Extracted PCR values, submitting on-chain"
    );

    // 3. Submit on-chain.
    chain
        .register_enclave(
            validator_uid,
            attestation_document,
            enclave_pubkey,
            pcr0,
            pcr1,
            pcr2,
        )
        .await?;

    tracing::info!("Enclave registered successfully");
    Ok(())
}

/// Extract PCR0, PCR1, PCR2 from a Nitro attestation document.
///
/// The document is a COSE_Sign1 structure:
///   CBOR array: [protected_header_bstr, unprotected_map, payload_bstr, signature_bstr]
///   (optionally tagged with CBOR tag 18 for COSE_Sign1)
///
/// The payload is a CBOR map with key "pcrs" → map of int → bytes (48 bytes each).
pub fn extract_pcrs(attestation_doc: &[u8]) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
    let value: Value = ciborium::from_reader(attestation_doc)
        .map_err(|e| anyhow::anyhow!("CBOR parse error: {e}"))?;

    // Unwrap COSE_Sign1 tag (18) if present, then get the array
    let array = unwrap_cose_sign1(value)?;

    if array.len() != 4 {
        anyhow::bail!("COSE_Sign1 must have 4 elements, got {}", array.len());
    }

    // Element 2 is the payload bstr
    let payload_bytes = match &array[2] {
        Value::Bytes(b) => b.clone(),
        other => anyhow::bail!("COSE payload (element 2) must be bstr, got {:?}", other),
    };

    // Parse the payload CBOR map
    let payload: Value = ciborium::from_reader(payload_bytes.as_slice())
        .map_err(|e| anyhow::anyhow!("attestation payload CBOR error: {e}"))?;

    let map = match payload {
        Value::Map(m) => m,
        _ => anyhow::bail!("attestation payload must be a CBOR map"),
    };

    // Find the "pcrs" key
    let pcrs_value = map
        .iter()
        .find(|(k, _)| matches!(k, Value::Text(s) if s == "pcrs"))
        .map(|(_, v)| v)
        .ok_or_else(|| anyhow::anyhow!("missing 'pcrs' field in attestation payload"))?;

    let pcrs_map = match pcrs_value {
        Value::Map(m) => m,
        _ => anyhow::bail!("pcrs field must be a CBOR map"),
    };

    let pcr0 = get_pcr_bytes(pcrs_map, 0)?;
    let pcr1 = get_pcr_bytes(pcrs_map, 1)?;
    let pcr2 = get_pcr_bytes(pcrs_map, 2)?;

    // Validate: each PCR must be exactly 48 bytes (SHA-384)
    for (pcr, idx) in [(&pcr0, 0u8), (&pcr1, 1), (&pcr2, 2)] {
        if pcr.len() != 48 {
            anyhow::bail!("PCR{idx} must be 48 bytes, got {} bytes", pcr.len());
        }
    }

    Ok((pcr0, pcr1, pcr2))
}

fn unwrap_cose_sign1(value: Value) -> Result<Vec<Value>> {
    match value {
        // COSE_Sign1 is typically tagged with tag 18
        Value::Tag(18, inner) => match *inner {
            Value::Array(arr) => Ok(arr),
            _ => anyhow::bail!("COSE_Sign1 tag must wrap an array"),
        },
        Value::Array(arr) => Ok(arr),
        _ => anyhow::bail!("expected COSE_Sign1 array or tagged array"),
    }
}

fn get_pcr_bytes(pcrs_map: &[(Value, Value)], index: u64) -> Result<Vec<u8>> {
    pcrs_map
        .iter()
        .find(|(k, _)| {
            // PCR index can be encoded as integer or as text (e.g., "0")
            match k {
                Value::Integer(i) => i128::from(*i) == index as i128,
                Value::Text(s) => s.parse::<u64>().ok() == Some(index),
                _ => false,
            }
        })
        .and_then(|(_, v)| {
            if let Value::Bytes(b) = v {
                Some(b.clone())
            } else {
                None
            }
        })
        .ok_or_else(|| anyhow::anyhow!("PCR{index} not found in attestation"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_cbor(value: &Value) -> Vec<u8> {
        let mut buf = Vec::new();
        ciborium::into_writer(value, &mut buf).unwrap();
        buf
    }

    fn make_mock_attestation() -> Vec<u8> {
        let pcr0 = vec![1u8; 48];
        let pcr1 = vec![2u8; 48];
        let pcr2 = vec![3u8; 48];

        // Build the pcrs map: {0: pcr0, 1: pcr1, 2: pcr2}
        let pcrs_map = Value::Map(vec![
            (Value::Integer(0u8.into()), Value::Bytes(pcr0)),
            (Value::Integer(1u8.into()), Value::Bytes(pcr1)),
            (Value::Integer(2u8.into()), Value::Bytes(pcr2)),
        ]);

        // Build the payload map
        let payload_map = Value::Map(vec![
            (
                Value::Text("module_id".to_string()),
                Value::Text("test".to_string()),
            ),
            (
                Value::Text("timestamp".to_string()),
                Value::Integer(0u64.into()),
            ),
            (Value::Text("pcrs".to_string()), pcrs_map),
        ]);

        // Encode payload
        let payload_bytes = encode_cbor(&payload_map);

        // Build COSE_Sign1 array: [protected, unprotected, payload_bstr, signature]
        let cose = Value::Array(vec![
            Value::Bytes(vec![]),        // protected
            Value::Map(vec![]),          // unprotected
            Value::Bytes(payload_bytes), // payload
            Value::Bytes(vec![0u8; 64]), // signature (mock)
        ]);

        encode_cbor(&cose)
    }

    #[test]
    fn extract_pcrs_from_mock_attestation() {
        let doc = make_mock_attestation();
        let (pcr0, pcr1, pcr2) = extract_pcrs(&doc).unwrap();
        assert_eq!(pcr0, vec![1u8; 48]);
        assert_eq!(pcr1, vec![2u8; 48]);
        assert_eq!(pcr2, vec![3u8; 48]);
    }

    #[test]
    fn extract_pcrs_with_tag_18() {
        let doc_bytes = make_mock_attestation();
        // Wrap in CBOR tag 18
        let inner: Value = ciborium::from_reader(doc_bytes.as_slice()).unwrap();
        let tagged = Value::Tag(18, Box::new(inner));
        let tagged_bytes = encode_cbor(&tagged);

        let (pcr0, _, _) = extract_pcrs(&tagged_bytes).unwrap();
        assert_eq!(pcr0, vec![1u8; 48]);
    }

    #[test]
    fn invalid_pcr_length_fails() {
        // Build with 32-byte PCRs (wrong, should be 48)
        let pcrs_map = Value::Map(vec![
            (Value::Integer(0u8.into()), Value::Bytes(vec![0u8; 32])),
            (Value::Integer(1u8.into()), Value::Bytes(vec![0u8; 32])),
            (Value::Integer(2u8.into()), Value::Bytes(vec![0u8; 32])),
        ]);
        let payload_map = Value::Map(vec![(Value::Text("pcrs".to_string()), pcrs_map)]);
        let payload_bytes = encode_cbor(&payload_map);
        let cose = Value::Array(vec![
            Value::Bytes(vec![]),
            Value::Map(vec![]),
            Value::Bytes(payload_bytes),
            Value::Bytes(vec![]),
        ]);
        let doc = encode_cbor(&cose);

        assert!(extract_pcrs(&doc).is_err());
    }
}
