// src/plugins/storage/providers/local.rs
//
// LocalStoragePlugin — the default backend.
//
// Files are already on disk after upload/transcode, so put_file and put_dir
// are no-ops. get_url constructs a URL relative to BASE_URL. get_to_file
// copies from the configured base_path (used when another module needs to
// fetch a file it did not write itself).
//
// Env vars:
//   STORAGE_LOCAL_PATH   base directory for local storage (default: "storage")
//   BASE_URL             public root URL for constructing links (default: http://localhost:8080)

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::Path;

use crate::plugins::storage::traits::StoragePlugin;

pub struct LocalStoragePlugin {
    base_path: String,
    #[allow(dead_code)]
    base_url: String,
}

impl LocalStoragePlugin {
    pub fn from_env() -> Self {
        Self {
            base_path: std::env::var("STORAGE_LOCAL_PATH").unwrap_or_else(|_| "storage".into()),
            base_url: std::env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:8080".into()),
        }
    }

    fn full_path(&self, key: &str) -> std::path::PathBuf {
        Path::new(&self.base_path).join(key)
    }
}

#[async_trait]
impl StoragePlugin for LocalStoragePlugin {
    fn backend_name(&self) -> &'static str {
        "local"
    }
    fn is_local(&self) -> bool {
        true
    }

    // Files live on the local filesystem already — nothing to upload.
    async fn put_file(&self, _key: &str, _path: &Path) -> Result<()> {
        Ok(())
    }
    async fn put_dir(&self, _key_prefix: &str, _local_dir: &Path) -> Result<usize> {
        Ok(0)
    }

    async fn get_url(&self, key: &str) -> String {
        format!("{}/storage/{}", self.base_url.trim_end_matches('/'), key)
    }

    async fn get_to_file(&self, key: &str, dest: &Path) -> Result<()> {
        let src = self.full_path(key);
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::copy(&src, dest)
            .await
            .with_context(|| format!("local copy {} → {}", src.display(), dest.display()))?;
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let path = self.full_path(key);
        if path.exists() {
            tokio::fs::remove_file(&path)
                .await
                .with_context(|| format!("local delete {}", path.display()))?;
        }
        Ok(())
    }
}
