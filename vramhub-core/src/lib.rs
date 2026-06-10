// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2025 VRAM AI Limited

//! # vramhub-core
//!
//! Shared types, constants, and utilities for the Sui LLM Coordination Layer.
//! No dependencies on Sui SDK or any I/O - pure data types and logic only.

pub mod constants;
pub mod errors;
pub mod openskill;
pub mod types;

pub use errors::VramhubError;
pub use types::*;
