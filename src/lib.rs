// src/lib.rs
//
// Library entry point for reusable application modules.
//
// The binary application still starts from `src/main.rs`. This file exposes the
// plugin architecture so payment providers can be developed and tested without
// forcing the existing HTTP payment flow to migrate immediately.

pub mod plugins;
pub mod payment_settings;
