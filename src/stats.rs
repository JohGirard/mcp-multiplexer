use crate::cache::{cache_dir, cache_path};
use crate::model::ToolInfo;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use tokio::sync::Mutex;

/// Usage counters for the `mcp-multiplexer --stats` report. Whole file
/// rewritten on every change (it's a few hundred bytes); same tmp+rename
/// pattern as tokens.json.
/// ponytail: counters reset only if the file is deleted — no per-day history.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct StatsData {
    /// Context cost the mux itself adds at startup (its own tool list, bytes).
    #[serde(default)]
    pub meta_bytes: u64,
    /// Per meta-tool call counts.
    #[serde(default)]
    pub meta_calls: BTreeMap<String, u64>,
    /// Schema bytes served on demand via search_tools/describe_tool.
    #[serde(default)]
    pub schema_bytes_served: u64,
    /// Proxied upstream calls (call_tool + exposed routes).
    #[serde(default)]
    pub tool_calls_proxied: u64,
}

pub struct Stats {
    data: Mutex<StatsData>,
    path: PathBuf,
}

impl Stats {
    pub fn new(meta_bytes: u64) -> std::sync::Arc<Stats> {
        let path = cache_dir().join("stats.json");
        let mut data: StatsData = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        data.meta_bytes = meta_bytes;
        let s = std::sync::Arc::new(Stats {
            data: Mutex::new(data),
            path,
        });
        s.persist_sync();
        s
    }

    fn persist_sync(&self) {
        let data = match self.data.try_lock() {
            Ok(d) => d,
            Err(_) => return, // a writer is mid-update; its save will cover us
        };
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = self.path.with_extension("json.tmp");
        if let Ok(json) = serde_json::to_string(&*data) {
            let _ = std::fs::write(&tmp, json);
            let _ = std::fs::rename(&tmp, &self.path);
        }
    }

    pub async fn record_meta(&self, tool: &str) {
        {
            let mut d = self.data.lock().await;
            *d.meta_calls.entry(tool.to_string()).or_insert(0) += 1;
        }
        self.persist_sync();
    }

    pub async fn record_served(&self, schema_bytes: u64) {
        {
            let mut d = self.data.lock().await;
            d.schema_bytes_served += schema_bytes;
        }
        self.persist_sync();
    }

    pub async fn record_proxied(&self) {
        {
            let mut d = self.data.lock().await;
            d.tool_calls_proxied += 1;
        }
        self.persist_sync();
    }
}

fn json_len<T: Serialize>(v: &T) -> u64 {
    serde_json::to_string(v)
        .map(|s| s.len() as u64)
        .unwrap_or(0)
}

/// rtk-gain-style report. Reads the cached index (what a direct connection
/// would inject) plus the counters file; works without a running mux.
pub fn report() -> String {
    let index_text = std::fs::read_to_string(cache_path()).ok();
    let tools: Vec<ToolInfo> = index_text
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v.get("servers")?.as_object().cloned())
        .map(|servers| {
            servers
                .values()
                .filter_map(|ts| serde_json::from_value::<Vec<ToolInfo>>(ts.clone()).ok())
                .flatten()
                .collect()
        })
        .unwrap_or_default();
    let stats: StatsData = std::fs::read_to_string(cache_dir().join("stats.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();

    let mut out = String::from("mcp-multiplexer stats\n\n");
    if tools.is_empty() {
        out.push_str("No cached index yet — start the mux once to build it.\n");
    } else {
        let withheld: u64 = tools.iter().map(json_len).sum();
        let saved = withheld.saturating_sub(stats.meta_bytes);
        let pct = (saved * 100).checked_div(withheld).unwrap_or(0);
        out.push_str(&format!(
            "Startup context per session:\n  without mux: ~{} tokens ({} tools)\n  with mux:    ~{} tokens (meta-tools)\n  saved:       ~{} tokens ({}%)\n",
            withheld / 4,
            tools.len(),
            stats.meta_bytes / 4,
            saved / 4,
            pct
        ));
    }
    let total_meta: u64 = stats.meta_calls.values().sum();
    out.push_str(&format!(
        "\nOn-demand schemas served: ~{} tokens\nTool calls proxied: {}\nMeta-tool calls: {}\n",
        stats.schema_bytes_served / 4,
        stats.tool_calls_proxied,
        total_meta
    ));
    if total_meta > 0 {
        for (name, n) in &stats.meta_calls {
            out.push_str(&format!("  {name}: {n}\n"));
        }
    }
    out.push_str("\nTokens ~ bytes/4 (heuristic, not a real tokenizer).\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stats_roundtrip_and_counters() {
        let dir = std::env::temp_dir().join(format!("mcpmux-stats-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("stats.json");
        let s = Stats {
            data: Mutex::new(StatsData::default()),
            path: path.clone(),
        };
        s.record_meta("search_tools").await;
        s.record_meta("search_tools").await;
        s.record_served(500).await;
        s.record_proxied().await;
        let loaded: StatsData =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded.meta_calls["search_tools"], 2);
        assert_eq!(loaded.schema_bytes_served, 500);
        assert_eq!(loaded.tool_calls_proxied, 1);
        std::fs::remove_dir_all(&dir).ok();
    }
}
