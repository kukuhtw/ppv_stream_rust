// src/plugins/storage/providers/s3.rs
//
// S3StoragePlugin — works with AWS S3, MinIO, Cloudflare R2, Backblaze B2,
// and any S3-compatible object store via the `object_store` crate.
//
// Required env vars:
//   S3_BUCKET          bucket name
//
// Optional env vars:
//   S3_REGION          AWS region (default: us-east-1)
//   S3_ACCESS_KEY      access key ID
//   S3_SECRET_KEY      secret access key
//   S3_ENDPOINT        custom endpoint URL (for MinIO/R2/B2 — e.g. http://minio:9000)
//   S3_PATH_STYLE      "true" to force path-style URLs (required by MinIO)
//   S3_PUBLIC_URL      base URL for constructing public object links (overrides endpoint-derived URL)
//   S3_PRESIGN_SECS    reserved — not used in current implementation
//
// STORAGE_BACKEND values that map to this provider:
//   s3, minio, r2, b2

use anyhow::{Context, Result};
use async_trait::async_trait;
use bytes::Bytes;
use object_store::path::Path as ObjPath;
use object_store::{aws::AmazonS3Builder, ObjectStore, PutPayload};
use std::path::Path;
use std::sync::Arc;

use crate::plugins::storage::traits::StoragePlugin;

pub struct S3StoragePlugin {
    store: Arc<dyn ObjectStore>,
    bucket: String,
    region: String,
    endpoint: String,
    public_url: Option<String>,
}

impl S3StoragePlugin {
    pub fn from_parts(
        store: Arc<dyn ObjectStore>,
        bucket: String,
        region: String,
        endpoint: String,
        public_url: Option<String>,
    ) -> Self {
        Self {
            store,
            bucket,
            region,
            endpoint,
            public_url,
        }
    }

    pub fn from_env() -> Result<Self> {
        let bucket = std::env::var("S3_BUCKET")
            .context("S3_BUCKET is required for s3/minio storage backend")?;
        let region = std::env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".into());
        let access_key = std::env::var("S3_ACCESS_KEY").unwrap_or_default();
        let secret_key = std::env::var("S3_SECRET_KEY").unwrap_or_default();
        let endpoint = std::env::var("S3_ENDPOINT").unwrap_or_default();
        let path_style = std::env::var("S3_PATH_STYLE").ok().as_deref() == Some("true");
        let public_url = std::env::var("S3_PUBLIC_URL").ok();

        let mut builder = AmazonS3Builder::new()
            .with_bucket_name(&bucket)
            .with_region(&region);

        if !access_key.is_empty() {
            builder = builder.with_access_key_id(&access_key);
        }
        if !secret_key.is_empty() {
            builder = builder.with_secret_access_key(&secret_key);
        }
        if !endpoint.is_empty() {
            builder = builder.with_endpoint(&endpoint);
            if path_style {
                builder = builder.with_virtual_hosted_style_request(false);
            }
            if endpoint.starts_with("http://") {
                builder = builder.with_allow_http(true);
            }
        }

        let store = builder
            .build()
            .context("failed to build S3 object store client")?;

        Ok(Self::from_parts(
            Arc::new(store),
            bucket,
            region,
            endpoint,
            public_url,
        ))
    }

    /// Human-readable description of the remote endpoint (for logs).
    pub fn endpoint_display(&self) -> String {
        if !self.endpoint.is_empty() {
            format!("{}/{}", self.endpoint.trim_end_matches('/'), self.bucket)
        } else {
            format!("s3://{}.s3.{}.amazonaws.com", self.bucket, self.region)
        }
    }

    fn obj_path(key: &str) -> ObjPath {
        ObjPath::from(key)
    }

    /// Recursively collect all file paths under `dir` using async readdir.
    async fn walk_dir(dir: &Path) -> Result<Vec<std::path::PathBuf>> {
        let mut files = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(current) = stack.pop() {
            let mut entries = tokio::fs::read_dir(&current)
                .await
                .with_context(|| format!("read_dir {}", current.display()))?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                let meta = entry.metadata().await?;
                if meta.is_dir() {
                    stack.push(path);
                } else {
                    files.push(path);
                }
            }
        }
        Ok(files)
    }
}

#[async_trait]
impl StoragePlugin for S3StoragePlugin {
    fn backend_name(&self) -> &'static str {
        "s3"
    }
    fn is_local(&self) -> bool {
        false
    }

    async fn put_file(&self, key: &str, path: &Path) -> Result<()> {
        let data: Vec<u8> = tokio::fs::read(path)
            .await
            .with_context(|| format!("read local file {}", path.display()))?;
        let payload = PutPayload::from(Bytes::from(data));
        self.store
            .put(&Self::obj_path(key), payload)
            .await
            .with_context(|| format!("S3 put {key}"))?;
        Ok(())
    }

    async fn put_dir(&self, key_prefix: &str, local_dir: &Path) -> Result<usize> {
        let files = Self::walk_dir(local_dir).await?;
        let count = files.len();
        for path in files {
            let rel = path
                .strip_prefix(local_dir)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/"); // Windows path separator
            let key = format!("{key_prefix}/{rel}");
            self.put_file(&key, &path).await?;
        }
        Ok(count)
    }

    async fn get_url(&self, key: &str) -> String {
        // 1. Explicit public/CDN URL (highest priority)
        if let Some(base) = &self.public_url {
            return format!("{}/{key}", base.trim_end_matches('/'));
        }
        // 2. Custom endpoint (MinIO path-style)
        if !self.endpoint.is_empty() {
            return format!(
                "{}/{}/{key}",
                self.endpoint.trim_end_matches('/'),
                self.bucket
            );
        }
        // 3. AWS S3 virtual-hosted-style
        format!(
            "https://{}.s3.{}.amazonaws.com/{key}",
            self.bucket, self.region
        )
    }

    async fn get_to_file(&self, key: &str, dest: &Path) -> Result<()> {
        let result = self
            .store
            .get(&Self::obj_path(key))
            .await
            .with_context(|| format!("S3 get {key}"))?;
        let data = result
            .bytes()
            .await
            .with_context(|| format!("S3 read bytes for {key}"))?;
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(dest, data)
            .await
            .with_context(|| format!("write to {}", dest.display()))?;
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.store
            .delete(&Self::obj_path(key))
            .await
            .with_context(|| format!("S3 delete {key}"))?;
        Ok(())
    }
}
