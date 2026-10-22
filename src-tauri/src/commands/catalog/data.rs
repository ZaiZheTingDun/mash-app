//! Resource-pack catalog loading and generation-aware read caches.
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

static ASSETS_ROOT: OnceLock<PathBuf> = OnceLock::new();
static GENERATION: AtomicU64 = AtomicU64::new(0);

pub(crate) fn initialize_catalogs(assets_root: PathBuf) {
    let _ = ASSETS_ROOT.set(assets_root);
    invalidate_catalogs();
}

pub(crate) fn invalidate_catalogs() {
    GENERATION.fetch_add(1, Ordering::AcqRel);
}

pub(super) struct CatalogCache<T>(Mutex<Option<(u64, Arc<T>)>>);

impl<T> CatalogCache<T> {
    pub(super) const fn new() -> Self {
        Self(Mutex::new(None))
    }

    pub(super) fn get_or_init(&self, build: impl FnOnce() -> T) -> Arc<T> {
        self.get_or_try_init(|| Ok(build()))
            .expect("infallible catalog cache")
    }

    pub(super) fn get_or_try_init(
        &self,
        build: impl FnOnce() -> Result<T, String>,
    ) -> Result<Arc<T>, String> {
        self.get_at(GENERATION.load(Ordering::Acquire), build)
    }

    fn get_at(
        &self,
        generation: u64,
        build: impl FnOnce() -> Result<T, String>,
    ) -> Result<Arc<T>, String> {
        let mut cache = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if let Some((cached_generation, data)) = cache.as_ref() {
            if *cached_generation == generation {
                return Ok(Arc::clone(data));
            }
        }
        let data = Arc::new(build()?);
        *cache = Some((generation, Arc::clone(&data)));
        Ok(data)
    }
}

pub(super) fn catalog_json<T: DeserializeOwned>(name: &str) -> Result<T, String> {
    let root = catalogs_root()?;
    catalog_json_at(&root, name)
}

fn catalogs_root() -> Result<PathBuf, String> {
    if let Some(root) = ASSETS_ROOT.get() {
        return Ok(root.join("catalogs"));
    }
    // Offline unit tests read explicit snapshots; production requires installed assets.
    #[cfg(test)]
    {
        Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/catalogs"))
    }
    #[cfg(not(test))]
    {
        Err("资源目录尚未初始化".to_string())
    }
}

fn catalog_json_at<T: DeserializeOwned>(root: &Path, name: &str) -> Result<T, String> {
    let path = root.join(name);
    let raw = std::fs::read_to_string(&path).map_err(|error| {
        format!(
            "读取汇总资源失败 ({}): {error}，请重新下载资源包",
            path.display()
        )
    })?;
    serde_json::from_str(&raw).map_err(|error| {
        format!(
            "解析汇总资源失败 ({}): {error}，请重新下载资源包",
            path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogs_require_installed_valid_files_without_a_bundled_fallback() {
        let temp = tempfile::tempdir().unwrap();
        for name in [
            "servants.json",
            "servants_variants.json",
            "servants_variants_cn.json",
            "craft_essences.json",
            "events.json",
            "event_items.json",
        ] {
            let read = || catalog_json_at::<Vec<u32>>(temp.path(), name);
            let error = read().unwrap_err();
            assert!(error.contains(name));
            assert!(error.contains("重新下载资源包"));
            std::fs::write(temp.path().join(name), "[19]").unwrap();
            assert_eq!(read().unwrap(), vec![19]);
            for invalid in ["invalid", "{}"] {
                std::fs::write(temp.path().join(name), invalid).unwrap();
                assert!(read().unwrap_err().contains("解析汇总资源失败"));
            }
        }
    }

    #[test]
    fn cache_refreshes_after_install_and_retries_failed_loads() {
        let cache = CatalogCache::new();
        assert!(cache.get_at(0, || Err("missing file".into())).is_err());
        let old = cache.get_at(0, || Ok(vec![19])).unwrap();
        assert!(Arc::ptr_eq(
            &old,
            &cache.get_at(0, || panic!("cache miss")).unwrap()
        ));
        assert!(cache.get_at(1, || Err("invalid file".into())).is_err());
        let new = cache.get_at(1, || Ok(vec![20])).unwrap();
        assert_eq!(*old, vec![19]);
        assert_eq!(*new, vec![20]);
    }
}
