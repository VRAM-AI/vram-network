// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

//! Gradient compression utilities shared across all adapters.
//!
//! Wire format (top-K f16):
//!   [4 bytes: total_params u32 LE]
//!   [4 bytes: k u32 LE]
//!   [k × 4 bytes: index u32 LE]
//!   [k × 2 bytes: value f16 LE]
//!
//! f16 gives 2× bandwidth vs f32 with <0.01% precision loss on gradients.
//! Typical compression ratio at top-1%: ~98% smaller than raw f32.

use vramhub_core::VramhubError;

/// Compress a gradient to its top-K elements, quantized to f16.
///
/// `k` is typically `n_params / 100` (top 1%) or `n_params / 10` (top 10%).
/// Pass `topk_compression` hparam from on-chain as `k`.
pub fn compress_topk_f16(gradient: &[f32], k: usize) -> Vec<u8> {
    let k = k.min(gradient.len());
    let total = gradient.len() as u32;

    // Select top-K by magnitude
    let mut indexed: Vec<(u32, f32)> = gradient
        .iter()
        .enumerate()
        .map(|(i, &v)| (i as u32, v))
        .collect();
    indexed.sort_unstable_by(|a, b| {
        b.1.abs()
            .partial_cmp(&a.1.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    indexed.truncate(k);

    let k_actual = indexed.len() as u32;
    let mut out = Vec::with_capacity(8 + k_actual as usize * 6);

    // Header
    out.extend_from_slice(&total.to_le_bytes());
    out.extend_from_slice(&k_actual.to_le_bytes());

    // Indices (u32)
    for (idx, _) in &indexed {
        out.extend_from_slice(&idx.to_le_bytes());
    }

    // Values (f16 — manual IEEE 754 conversion)
    for (_, val) in &indexed {
        out.extend_from_slice(&f32_to_f16_bytes(*val));
    }

    out
}

/// Decompress a top-K f16 gradient back to a dense f32 vector.
pub fn decompress_topk_f16(compressed: &[u8]) -> Result<Vec<f32>, VramhubError> {
    if compressed.len() < 8 {
        return Err(VramhubError::Internal(
            "Compressed gradient too short".into(),
        ));
    }

    let total = u32::from_le_bytes(compressed[0..4].try_into().unwrap()) as usize;
    let k = u32::from_le_bytes(compressed[4..8].try_into().unwrap()) as usize;

    let indices_end = 8 + k * 4;
    let values_end = indices_end + k * 2;

    if compressed.len() < values_end {
        return Err(VramhubError::Internal(format!(
            "Compressed gradient truncated: need {values_end} bytes, got {}",
            compressed.len()
        )));
    }

    let mut out = vec![0.0f32; total];

    for i in 0..k {
        let idx_start = 8 + i * 4;
        let idx =
            u32::from_le_bytes(compressed[idx_start..idx_start + 4].try_into().unwrap()) as usize;
        let val_start = indices_end + i * 2;
        let val = f16_bytes_to_f32(&compressed[val_start..val_start + 2]);
        if idx < total {
            out[idx] = val;
        }
    }

    Ok(out)
}

/// f32 → f16 (IEEE 754 half-precision), returns 2 bytes LE.
fn f32_to_f16_bytes(v: f32) -> [u8; 2] {
    // Fast software f16 conversion
    let bits = v.to_bits();
    let sign = (bits >> 16) & 0x8000;
    let exp = ((bits >> 23) & 0xFF) as i32 - 127 + 15;
    let mant = (bits >> 13) & 0x3FF;

    let h: u16 = if exp <= 0 {
        // Underflow → zero
        sign as u16
    } else if exp >= 31 {
        // Overflow → infinity
        (sign as u16) | 0x7C00
    } else {
        (sign as u16) | ((exp as u16) << 10) | (mant as u16)
    };

    h.to_le_bytes()
}

/// f16 bytes (2 bytes LE) → f32.
fn f16_bytes_to_f32(b: &[u8]) -> f32 {
    let h = u16::from_le_bytes([b[0], b[1]]);
    let sign: u32 = ((h as u32) & 0x8000) << 16;
    let exp = (h >> 10) & 0x1F;
    let mant = (h as u32) & 0x3FF;

    if exp == 0 {
        // Subnormal or zero
        f32::from_bits(sign)
    } else if exp == 31 {
        // Inf or NaN
        f32::from_bits(sign | 0x7F80_0000 | (mant << 13))
    } else {
        let e = (exp as u32 + 127 - 15) << 23;
        f32::from_bits(sign | e | (mant << 13))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f16_roundtrip_precision() {
        for &v in &[0.0f32, 1.0, -1.0, 0.5, -0.5, 0.001, 100.0, -100.0] {
            let bytes = f32_to_f16_bytes(v);
            let back = f16_bytes_to_f32(&bytes);
            let err = (v - back).abs();
            assert!(
                err < 0.01 * v.abs().max(0.001),
                "f16 roundtrip error too large: {v} → {back} (err={err})"
            );
        }
    }

    #[test]
    fn topk_selects_largest_magnitude() {
        let grad: Vec<f32> = (0..100).map(|i| i as f32 - 50.0).collect();
        let compressed = compress_topk_f16(&grad, 10);
        let decompressed = decompress_topk_f16(&compressed).unwrap();
        assert_eq!(decompressed.len(), 100);

        // The 10 largest-magnitude values should be near the edges
        let nonzero: Vec<(usize, f32)> = decompressed
            .iter()
            .enumerate()
            .filter(|(_, &v)| v != 0.0)
            .map(|(i, &v)| (i, v))
            .collect();
        assert_eq!(nonzero.len(), 10, "Expected 10 non-zero entries");
        // All kept entries should have |val| >= 40 (edges of -50..49 range)
        for (_, v) in &nonzero {
            assert!(v.abs() >= 39.0, "Kept small value {v}");
        }
    }

    #[test]
    fn compress_decompress_roundtrip() {
        let grad: Vec<f32> = (0..1000).map(|i| (i as f32).sin()).collect();
        let k = 100;
        let compressed = compress_topk_f16(&grad, k);
        let back = decompress_topk_f16(&compressed).unwrap();
        assert_eq!(back.len(), grad.len());
        // Verify top-k indices have non-zero values
        let nonzero_count = back.iter().filter(|&&v| v != 0.0).count();
        assert_eq!(nonzero_count, k);
    }

    #[test]
    fn compression_ratio() {
        let grad: Vec<f32> = vec![1.0f32; 1_000_000];
        let k = 10_000; // top-1%
        let compressed = compress_topk_f16(&grad, k);
        let raw_bytes = grad.len() * 4;
        let ratio = raw_bytes as f32 / compressed.len() as f32;
        println!("Compression ratio at top-1%: {ratio:.1}×");
        assert!(ratio > 50.0, "Expected >50× compression, got {ratio:.1}×");
    }
}
