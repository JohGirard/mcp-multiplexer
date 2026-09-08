pub use crate::model::ToolInfo;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Serialize, Deserialize)]
struct CacheFile {
    config_hash: u64,
    servers: BTreeMap<String, Vec<ToolInfo>>,
    instructions: BTreeMap<String, String>,
}

#[derive(Debug, Default)]
pub struct Cache {
    pub servers: BTreeMap<String, Vec<ToolInfo>>,
    pub instructions: BTreeMap<String, String>,
}

pub fn config_hash(text: &str) -> u64 {
    // ponytail: not cryptographic — cache key only, collisions just cause a cold start
    let mut h = DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

pub fn cache_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("mcp-multiplexer")
}

pub fn cache_path() -> PathBuf {
    cache_dir().join("index.json")
}

impl Cache {
    pub fn load(config_hash: u64) -> Cache {
        Self::load_from(&cache_path(), config_hash)
    }
    pub fn save(&self, config_hash: u64) -> anyhow::Result<()> {
        self.save_to(&cache_path(), config_hash)
    }

    pub fn load_from(path: &Path, config_hash: u64) -> Cache {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Cache::default();
        };
        let Ok(f) = serde_json::from_str::<CacheFile>(&text) else {
            return Cache::default();
        };
        if f.config_hash != config_hash {
            return Cache::default();
        }
        Cache {
            servers: f.servers,
            instructions: f.instructions,
        }
    }

    pub fn save_to(&self, path: &Path, config_hash: u64) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let f = CacheFile {
            config_hash,
            servers: self.servers.clone(),
            instructions: self.instructions.clone(),
        };
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string(&f)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}
