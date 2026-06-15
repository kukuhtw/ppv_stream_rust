// src/plugins/storage/registry.rs
//
// StorageRegistry reads STORAGE_BACKEND from the environment and constructs
// the matching StoragePlugin. Falls back to LocalStoragePlugin on misconfiguration.
//
// Env var:
//   STORAGE_BACKEND   "local" (default) | "s3" | "minio" | "r2" | "b2"

use std::sync::Arc;

use crate::plugins::storage::{
    providers::{local::LocalStoragePlugin, s3::S3StoragePlugin},
    traits::StoragePlugin,
};

pub struct StorageRegistry {
    backend: Arc<dyn StoragePlugin>,
}

impl StorageRegistry {
    pub fn from_env() -> Self {
        let name = std::env::var("STORAGE_BACKEND")
            .unwrap_or_else(|_| "local".into())
            .to_lowercase();

        let backend: Arc<dyn StoragePlugin> = match name.as_str() {
            "s3" | "minio" | "r2" | "b2" => match S3StoragePlugin::from_env() {
                Ok(p) => {
                    tracing::info!("storage backend: {} ({})", name, p.endpoint_display());
                    Arc::new(p)
                }
                Err(e) => {
                    tracing::error!(
                        "storage: {} init failed — {e}; falling back to local",
                        name
                    );
                    Arc::new(LocalStoragePlugin::from_env())
                }
            },
            _ => {
                tracing::info!("storage backend: local");
                Arc::new(LocalStoragePlugin::from_env())
            }
        };

        Self { backend }
    }

    /// Return a clone of the shared plugin handle.
    pub fn plugin(&self) -> Arc<dyn StoragePlugin> {
        self.backend.clone()
    }
}
