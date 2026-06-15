// src/plugins/storage/mod.rs
//
// Storage plugin system — abstracts file persistence over local disk, MinIO,
// AWS S3, Cloudflare R2, Backblaze B2, and any S3-compatible object store.
//
// Usage:
//   let registry = StorageRegistry::from_env();
//   let storage  = registry.plugin();   // Arc<dyn StoragePlugin>
//
// Configuration:
//   STORAGE_BACKEND=local           — local filesystem (default, no extra deps)
//   STORAGE_BACKEND=s3|minio|r2|b2  — S3-compatible (needs S3_BUCKET etc.)

pub mod providers;
pub mod registry;
pub mod traits;

pub use registry::StorageRegistry;
pub use traits::StoragePlugin;
