// src/plugins/storage/traits.rs
//
// StoragePlugin trait: abstraction over local disk, MinIO, AWS S3,
// Cloudflare R2, Backblaze B2, and any S3-compatible object store.

use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;

#[async_trait]
pub trait StoragePlugin: Send + Sync {
    /// Short identifier, e.g. "local", "s3", "minio".
    fn backend_name(&self) -> &'static str;

    /// Returns true when files already live on the local filesystem and
    /// uploading them to the backend is a no-op.
    fn is_local(&self) -> bool;

    /// Store a local file at `path` under the given object key.
    async fn put_file(&self, key: &str, path: &Path) -> Result<()>;

    /// Recursively store every file under `local_dir` with keys prefixed
    /// by `key_prefix/relative-path`. Returns the number of objects stored.
    async fn put_dir(&self, key_prefix: &str, local_dir: &Path) -> Result<usize>;

    /// Return a URL for serving `key` — a presigned URL, a public CDN URL,
    /// or a local HTTP path, depending on the backend configuration.
    async fn get_url(&self, key: &str) -> String;

    /// Download `key` from the backend and write it to `dest` on the local
    /// filesystem (used to fetch originals for FFmpeg re-processing).
    async fn get_to_file(&self, key: &str, dest: &Path) -> Result<()>;

    /// Delete `key` from the backend.
    async fn delete(&self, key: &str) -> Result<()>;
}
