// src/plugins/storage/providers/mod.rs
//
// Built-in storage providers:
//   local  — local filesystem (default, no extra config needed)
//   s3     — AWS S3, MinIO, Cloudflare R2, Backblaze B2 (set STORAGE_BACKEND=s3|minio|r2|b2)
//
// Adding a new provider (e.g. Google Cloud Storage):
//   1. Add `object_store` feature "gcs" to Cargo.toml
//   2. Create providers/gcs.rs implementing StoragePlugin
//   3. Add `pub mod gcs;` here
//   4. Handle "gcs" match arm in StorageRegistry::from_env()

pub mod local;
pub mod s3;
