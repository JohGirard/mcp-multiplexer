use crate::cache::Cache;
use crate::config::{Config, ServerConfig};
use crate::model::ToolInfo;
use crate::oauth::{OAuth, TokenStore};
use anyhow::{anyhow, bail};
use rmcp::ServiceExt;
use rmcp::model::CallToolResult;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::{Mutex, OnceCell, RwLock};

struct Entry {
    cfg: ServerConfig,
    // RwLock so a dead mid-session client can be taken out; OnceCell keeps single-connect semantics
    client: RwLock<OnceCell<Arc<rmcp::service::RunningService<rmcp::service::RoleClient, ()>>>>,
    failed: Mutex<Option<String>>,
    tools: Mutex<Vec<ToolInfo>>,
    instructions: Mutex<Option<String>>,
    oauth: Option<Arc<OAuth>>,
    token_store: Option<TokenStore>,
}

pub struct Upstreams {
    entries: BTreeMap<String, Arc<Entry>>,
    cache: Mutex<Cache>,
    config_hash: u64,
}

const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 10;

async fn connect(
    e: &Entry,
) -> anyhow::Result<rmcp::service::RunningService<rmcp::service::RoleClient, ()>> {
    let cfg = &e.cfg;
    let fut = async {
        if let Some(cmd) = &cfg.command {
            let mut c = tokio::process::Command::new(cmd);
            c.args(&cfg.args).envs(&cfg.env);
            let (transport, stderr) = rmcp::transport::TokioChildProcess::builder(c)
                .stderr(std::process::Stdio::piped())
                .spawn()?;
            if let Some(mut err) = stderr {
                use tokio::io::AsyncBufReadExt;
                tokio::spawn(async move {
                    let mut lines = tokio::io::BufReader::new(&mut err).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        tracing::debug!(target: "upstream-stderr", "{line}");
                    }
                });
            }
            Ok(().serve(transport).await?)
        } else if let Some(url) = &cfg.url {
            let mut config = rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig::with_uri(url.clone());
            for (k, v) in &cfg.headers {
                config.custom_headers.insert(
                    http::HeaderName::from_bytes(k.as_bytes())?,
                    http::HeaderValue::from_str(v)?,
                );
            }
            if let Some(oauth) = &e.oauth {
                let store = e.token_store.as_ref().unwrap();
                match oauth.access_token(cfg, store).await? {
                    Some(token) => {
                        config.custom_headers.insert(
                            http::header::AUTHORIZATION,
                            http::HeaderValue::from_str(&format!("Bearer {token}"))?,
                        );
                    }
                    None => {
                        let auth_url = oauth.begin_flow(cfg, store).await?;
                        bail!(
                            "server requires OAuth authorization. Open in a browser: {auth_url} — then retry. Headless? Call authorize_server with the final redirect URL as pasted_url."
                        );
                    }
                }
            }
            let transport = rmcp::transport::StreamableHttpClientTransport::from_config(config);
            Ok(().serve(transport).await?)
        } else {
            bail!("server has neither command nor url")
        }
    };
    let timeout =
        std::time::Duration::from_secs(cfg.connect_timeout.unwrap_or(DEFAULT_CONNECT_TIMEOUT_SECS));
    match tokio::time::timeout(timeout, fut).await {
        Ok(r) => r,
        Err(_) => Err(anyhow!("connect timed out after {}s", timeout.as_secs())),
    }
}

impl Upstreams {
    pub fn new(cfg: Config, cache: Cache, config_hash: u64) -> Upstreams {
        let token_lock = Arc::new(Mutex::new(()));
        let entries = cfg
            .mcp_servers
            .iter()
            .map(|(name, sc)| {
                let tools = cache.servers.get(name).cloned().unwrap_or_default();
                let instr = cache.instructions.get(name).cloned();
                (
                    name.clone(),
                    Arc::new(Entry {
                        cfg: sc.clone(),
                        client: RwLock::new(OnceCell::new()),
                        failed: Mutex::new(None),
                        tools: Mutex::new(tools),
                        instructions: Mutex::new(instr),
                        oauth: sc.oauth.then(OAuth::new),
                        token_store: sc.oauth.then(|| TokenStore::new(name, token_lock.clone())),
                    }),
                )
            })
            .collect();
        Upstreams {
            entries,
            cache: Mutex::new(cache),
            config_hash,
        }
    }

    pub fn server_names(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    pub fn status(&self, name: &str) -> &'static str {
        match self.entries.get(name) {
            None => "unknown",
            Some(e)
                if e.client
                    .try_read()
                    .map(|c| c.initialized())
                    .unwrap_or(false) =>
            {
                "connected"
            }
            Some(e) => match e.failed.try_lock() {
                Ok(g) if g.is_some() => "unavailable",
                _ => "cold",
            },
        }
    }

    pub fn instructions(&self, name: &str) -> Option<String> {
        self.entries
            .get(name)?
            .instructions
            .try_lock()
            .ok()?
            .clone()
    }

    fn entry(&self, name: &str) -> anyhow::Result<&Arc<Entry>> {
        self.entries.get(name).ok_or_else(|| {
            anyhow!(
                "unknown server {name:?}; available: {}",
                self.server_names().join(", ")
            )
        })
    }

    async fn ensure(
        &self,
        name: &str,
    ) -> anyhow::Result<Arc<rmcp::service::RunningService<rmcp::service::RoleClient, ()>>> {
        let e = self.entry(name)?;
        let cell = e.client.read().await;
        let r = cell
            .get_or_try_init(|| async {
                match connect(e).await {
                    Ok(c) => {
                        *e.failed.lock().await = None;
                        Ok(Arc::new(c))
                    }
                    Err(err) => {
                        *e.failed.lock().await = Some(err.to_string());
                        Err(err)
                    }
                }
            })
            .await?
            .clone();
        drop(cell);
        // after first connect, refresh index from live server
        if e.tools.lock().await.is_empty() {
            self.refresh_inner(name, &r).await.ok();
        }
        Ok(r)
    }

    pub async fn refresh(&self, name: &str) -> anyhow::Result<()> {
        let client = self.ensure(name).await?;
        self.refresh_inner(name, &client).await
    }

    async fn refresh_inner(
        &self,
        name: &str,
        client: &rmcp::service::RunningService<rmcp::service::RoleClient, ()>,
    ) -> anyhow::Result<()> {
        let e = self.entry(name)?;
        let listed = client.list_all_tools().await?;
        let mut out = Vec::new();
        for t in listed {
            if !e.cfg.is_allowed(&t.name) {
                continue;
            }
            out.push(ToolInfo {
                name: t.name.to_string(),
                description: t.description.map(|d| d.to_string()),
                schema: serde_json::to_value(&t.input_schema)?,
                annotations: t.annotations.map(serde_json::to_value).transpose()?,
            });
        }
        *e.tools.lock().await = out.clone();
        if let Some(info) = client.peer_info()
            && let Some(instr) = info.instructions.clone()
        {
            *e.instructions.lock().await = Some(instr.clone());
            self.cache
                .lock()
                .await
                .instructions
                .insert(name.to_string(), instr);
        }
        self.cache
            .lock()
            .await
            .servers
            .insert(name.to_string(), out);
        // persist right away — clients often SIGTERM us, so the shutdown save may never run
        self.save_cache().await;
        Ok(())
    }

    /// In-memory index only, never connects. Empty for cold servers.
    pub fn cached_tools(&self, name: &str) -> Vec<ToolInfo> {
        self.entries
            .get(name)
            .and_then(|e| e.tools.try_lock().ok().map(|g| g.clone()))
            .unwrap_or_default()
    }

    pub async fn tools(&self, name: &str) -> anyhow::Result<Vec<ToolInfo>> {
        let e = self.entry(name)?;
        let cached = e.tools.lock().await.clone();
        if !cached.is_empty() {
            return Ok(cached);
        }
        self.refresh(name).await?;
        Ok(e.tools.lock().await.clone())
    }

    pub async fn call(
        &self,
        name: &str,
        tool: &str,
        args: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> anyhow::Result<CallToolResult> {
        let e = self.entry(name)?;
        if !e.cfg.is_allowed(tool) {
            bail!("tool {tool:?} on server {name:?} is blocked by config");
        }
        let client = self.ensure(name).await?;
        let mut params = rmcp::model::CallToolRequestParams::new(tool.to_string());
        if let Some(a) = args {
            params = params.with_arguments(a);
        }
        match client.call_tool(params.clone()).await {
            Ok(r) => Ok(r),
            Err(err) => {
                // ponytail: heuristic — any call error drops the (possibly dead) client, then
                // reconnects, re-lists and retries exactly once; covers dead upstreams and stale index
                tracing::debug!(%err, "call failed, reconnecting and retrying once");
                e.client.write().await.take();
                let client = self.ensure(name).await?;
                self.refresh_inner(name, &client).await.ok();
                Ok(client.call_tool(params).await?)
            }
        }
    }

    /// Begin (or re-report) OAuth authorization for a server.
    /// Ok(None) = already authorized, Ok(Some) = URL the user must open.
    pub async fn oauth_begin(&self, name: &str) -> anyhow::Result<Option<String>> {
        let e = self.entry(name)?;
        let (Some(oauth), Some(store)) = (&e.oauth, &e.token_store) else {
            bail!("server {name:?} does not have \"oauth\": true in the config");
        };
        match oauth.access_token(&e.cfg, store).await? {
            Some(_) => Ok(None),
            None => Ok(Some(oauth.begin_flow(&e.cfg, store).await?)),
        }
    }

    /// Complete a headless OAuth flow from the pasted redirect URL.
    pub async fn oauth_complete(&self, name: &str, pasted: &str) -> anyhow::Result<()> {
        let e = self.entry(name)?;
        let Some(oauth) = &e.oauth else {
            bail!("server {name:?} does not have \"oauth\": true in the config");
        };
        oauth.complete_with_url(pasted).await
    }

    pub async fn save_cache(&self) {
        if let Err(e) = self.cache.lock().await.save(self.config_hash) {
            tracing::warn!(%e, "failed to save index cache");
        }
    }
}
