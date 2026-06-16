use anyhow::{anyhow, Context, Result};
use object_store::aws::AmazonS3Builder;
use serde::Serialize;
use sqlx::{PgPool, Row};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::plugins::storage::{
    providers::{local::LocalStoragePlugin, s3::S3StoragePlugin},
    traits::StoragePlugin,
};

#[derive(Clone, Debug, Serialize)]
pub struct StoredStorageSettings {
    pub backend: String,
    pub bucket: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
    pub endpoint: String,
    pub public_url: String,
    pub path_style: bool,
}

impl StoredStorageSettings {
    pub fn normalized_backend(&self) -> String {
        self.backend.trim().to_ascii_lowercase()
    }

    pub fn validate(&self) -> Result<()> {
        let backend = self.normalized_backend();
        match backend.as_str() {
            "local" => Ok(()),
            "s3" | "minio" | "r2" | "b2" => {
                if self.bucket.trim().is_empty() {
                    return Err(anyhow!("bucket is required for remote storage backends"));
                }
                Ok(())
            }
            _ => Err(anyhow!("backend must be one of: local, s3, minio, r2, b2")),
        }
    }

    pub fn missing_fields(&self) -> Vec<&'static str> {
        let backend = self.normalized_backend();
        if backend == "local" {
            return Vec::new();
        }
        let mut missing = Vec::new();
        if self.bucket.trim().is_empty() {
            missing.push("bucket");
        }
        missing
    }

    pub fn has_secret(&self) -> bool {
        !self.secret_key.is_empty()
    }

    pub fn build_plugin(&self) -> Result<Arc<dyn StoragePlugin>> {
        self.validate()?;
        let backend = self.normalized_backend();
        if backend == "local" {
            return Ok(Arc::new(LocalStoragePlugin::from_env()));
        }

        let mut builder = AmazonS3Builder::new()
            .with_bucket_name(&self.bucket)
            .with_region(if self.region.trim().is_empty() {
                "us-east-1"
            } else {
                self.region.trim()
            });

        if !self.access_key.trim().is_empty() {
            builder = builder.with_access_key_id(self.access_key.trim());
        }
        if !self.secret_key.trim().is_empty() {
            builder = builder.with_secret_access_key(self.secret_key.trim());
        }
        if !self.endpoint.trim().is_empty() {
            builder = builder.with_endpoint(self.endpoint.trim());
            if self.path_style {
                builder = builder.with_virtual_hosted_style_request(false);
            }
            if self.endpoint.trim().starts_with("http://") {
                builder = builder.with_allow_http(true);
            }
        }

        let store = builder
            .build()
            .context("failed to build S3-compatible object store client")?;

        Ok(Arc::new(S3StoragePlugin::from_parts(
            Arc::new(store),
            self.bucket.trim().to_string(),
            if self.region.trim().is_empty() {
                "us-east-1".to_string()
            } else {
                self.region.trim().to_string()
            },
            self.endpoint.trim().to_string(),
            if self.public_url.trim().is_empty() {
                None
            } else {
                Some(self.public_url.trim().to_string())
            },
        )))
    }
}

pub async fn load_storage_settings(pool: &PgPool) -> StoredStorageSettings {
    let row = sqlx::query(
        "SELECT backend, bucket, region, access_key, secret_key, endpoint, public_url, path_style
         FROM storage_settings WHERE id = TRUE",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    match row {
        Some(r) => StoredStorageSettings {
            backend: r
                .try_get::<String, _>("backend")
                .unwrap_or_else(|_| "local".into()),
            bucket: r.try_get::<String, _>("bucket").unwrap_or_default(),
            region: r
                .try_get::<String, _>("region")
                .unwrap_or_else(|_| "us-east-1".into()),
            access_key: r.try_get::<String, _>("access_key").unwrap_or_default(),
            secret_key: r.try_get::<String, _>("secret_key").unwrap_or_default(),
            endpoint: r.try_get::<String, _>("endpoint").unwrap_or_default(),
            public_url: r.try_get::<String, _>("public_url").unwrap_or_default(),
            path_style: r.try_get::<bool, _>("path_style").unwrap_or(false),
        },
        None => StoredStorageSettings {
            backend: "local".into(),
            bucket: String::new(),
            region: "us-east-1".into(),
            access_key: String::new(),
            secret_key: String::new(),
            endpoint: String::new(),
            public_url: String::new(),
            path_style: false,
        },
    }
}

pub async fn collect_local_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if !root.exists() {
        return Ok(files);
    }

    let mut stack = vec![root.to_path_buf()];
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

    files.sort();
    Ok(files)
}
