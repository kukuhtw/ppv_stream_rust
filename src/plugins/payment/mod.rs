// src/plugins/payment/mod.rs
//
// Payment plugin architecture.
//
// This module defines a provider-neutral payment abstraction so the platform can
// support PayPal, Stripe, Midtrans, Xendit, and x402 without hardcoding every
// provider directly inside HTTP handlers.

pub mod models;
pub mod registry;
pub mod traits;
pub mod providers;

pub use registry::PaymentPluginRegistry;
