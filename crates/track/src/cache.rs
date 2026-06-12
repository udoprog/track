use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context as _, Result};
use api::ImageSource;
use bytes::Bytes;
use lru::LruCache;
use parking_lot::Mutex;
use tokio::fs;
use tokio::io;
use tokio::sync::{RwLock, Semaphore};
use tokio::task::spawn_blocking;

const SHARDS: usize = 64;
const FETCH_LIMIT: usize = 8;
const MEMORY_CAPACITY: NonZeroUsize = NonZeroUsize::new(256).unwrap();

struct Inner {
    root: PathBuf,
    locks: Box<[RwLock<()>]>,
    semaphore: Semaphore,
    memory: Mutex<LruCache<String, Bytes>>,
}

#[derive(Clone)]
pub(super) struct ImageCache {
    inner: Arc<Inner>,
}

impl ImageCache {
    pub(super) fn new(root: impl AsRef<Path>) -> Self {
        Self {
            inner: Arc::new(Inner {
                root: root.as_ref().to_owned(),
                locks: (0..SHARDS).map(|_| RwLock::new(())).collect(),
                semaphore: Semaphore::new(FETCH_LIMIT),
                memory: Mutex::new(LruCache::new(MEMORY_CAPACITY)),
            }),
        }
    }

    fn disk_path(&self, source: ImageSource, path: &str) -> Option<PathBuf> {
        if path.contains("..") || path.starts_with('/') {
            return None;
        }

        Some(self.inner.root.join(source.as_str()).join(path))
    }

    fn shard(key: &str) -> usize {
        // FNV-1a hash for shard selection
        let mut h: u64 = 0xcbf29ce484222325;
        for b in key.bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        (h % SHARDS as u64) as usize
    }

    pub(super) async fn get_or_fetch(
        &self,
        source: ImageSource,
        path: &str,
        fetch: impl AsyncFnOnce() -> Result<Option<Bytes>>,
    ) -> Result<Option<Bytes>> {
        let key = format!("{source}/{path}");

        if let Some(data) = self.inner.memory.lock().get(&key).cloned() {
            return Ok(Some(data));
        }

        let disk_path = self.disk_path(source, path).context("invalid image path")?;

        let shard = Self::shard(&key);
        let lock = &self.inner.locks[shard];

        {
            let _guard = lock.read().await;
            match fs::read(&disk_path).await {
                Ok(data) => {
                    let bytes = Bytes::from(data);
                    self.inner.memory.lock().put(key.clone(), bytes.clone());
                    return Ok(Some(bytes));
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => tracing::warn!(%key, error = %e, "cache read error"),
            }
        }

        let _guard = lock.write().await;
        match fs::read(&disk_path).await {
            Ok(data) => {
                let bytes = Bytes::from(data);
                self.inner.memory.lock().put(key.clone(), bytes.clone());
                return Ok(Some(bytes));
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => tracing::warn!(%key, error = %e, "cache read error"),
        }

        let _permit = self
            .inner
            .semaphore
            .acquire()
            .await
            .context("acquiring fetch permit")?;

        let data = match fetch().await? {
            Some(d) => d,
            None => return Ok(None),
        };

        let parent = disk_path.parent().unwrap_or(&self.inner.root).to_owned();
        fs::create_dir_all(&parent)
            .await
            .context("creating cache dirs")?;

        let write_bytes = data.clone();
        let final_path = disk_path;
        if let Err(e) = spawn_blocking(move || {
            use std::io::Write as _;
            let mut tmp = tempfile::Builder::new().tempfile_in(&parent)?;
            tmp.write_all(&write_bytes)?;
            tmp.persist(&final_path)
                .map_err(|e| e.error)
                .context("persisting cache file")?;
            Ok::<_, anyhow::Error>(())
        })
        .await?
        {
            tracing::warn!(%key, error = %e, "cache write error");
        }

        self.inner.memory.lock().put(key, data.clone());
        Ok(Some(data))
    }
}
