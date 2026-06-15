// src/plugins/payment/env.rs
//
// Helper functions for reading provider-specific environment variables.

use std::env;

pub fn missing_env(required: &[&str]) -> Vec<String> {
    required
        .iter()
        .filter(|key| env::var(key).unwrap_or_default().trim().is_empty())
        .map(|key| key.to_string())
        .collect()
}

pub fn required_env(required: &[&str]) -> Vec<String> {
    required.iter().map(|key| key.to_string()).collect()
}

pub fn env_or(key: &str, default_value: &str) -> String {
    env::var(key).unwrap_or_else(|_| default_value.to_string())
}
