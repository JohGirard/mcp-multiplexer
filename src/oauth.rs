use crate::cache::cache_dir;
use crate::config::ServerConfig;
use anyhow::{anyhow, bail};
use rmcp::transport::auth::{
    AuthError, AuthorizationManager, AuthorizationMetadata, AuthorizationRequest,
    AuthorizationSession, CredentialStore, OAuthState, StoredCredentials,
};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Per-server view over `tokens.json` in the cache dir. The whole file is
/// rewritten on every save; `lock` (shared per process) serializes that.
/// ponytail: single-process tool — no cross-process refresh coordination.
#[derive(Clone)]
pub struct TokenStore {
    server: String,
    path: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl TokenStore {
    pub fn new(server: &str, lock: Arc<Mutex<()>>) -> TokenStore {
        TokenStore {
            server: server.into(),
            path: cache_dir().join("tokens.json"),
            lock,
        }
    }

    pub async fn stored_client_id(&self) -> Option<String> {
        CredentialStore::load(self)
            .await
            .ok()
            .flatten()
            .map(|c| c.client_id)
    }

    fn read_all(&self) -> BTreeMap<String, StoredCredentials> {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    fn write_all(&self, m: &BTreeMap<String, StoredCredentials>) -> Result<(), AuthError> {
        let io = |e: std::io::Error| AuthError::InternalError(e.to_string());
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(io)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        let json = serde_json::to_string(m).map_err(|e| AuthError::InternalError(e.to_string()))?;
        std::fs::write(&tmp, json).map_err(io)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600)).map_err(io)?;
        }
        std::fs::rename(&tmp, &self.path).map_err(io)?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl CredentialStore for TokenStore {
    async fn load(&self) -> Result<Option<StoredCredentials>, AuthError> {
        let _g = self.lock.lock().await;
        Ok(self.read_all().remove(&self.server))
    }
    async fn save(&self, credentials: StoredCredentials) -> Result<(), AuthError> {
        let _g = self.lock.lock().await;
        let mut all = self.read_all();
        all.insert(self.server.clone(), credentials);
        self.write_all(&all)
    }
    async fn clear(&self) -> Result<(), AuthError> {
        let _g = self.lock.lock().await;
        let mut all = self.read_all();
        all.remove(&self.server);
        self.write_all(&all)
    }
}

/// rmcp enforces RFC 8414 §3.3: the discovered `issuer` must match the host the
/// metadata was fetched from. Some providers delegate auth to a different
/// domain than the MCP endpoint — the resource lives on one host, the declared
/// `issuer` on another — and fail that check despite serving valid metadata.
/// When strict discovery rejects the mismatch, re-fetch the same well-known
/// candidates trusting the issuer the document itself declares.
/// Same candidate order as rmcp's `generate_discovery_urls`.
async fn discover_metadata_trusting_issuer(base: &str) -> anyhow::Result<AuthorizationMetadata> {
    let base: reqwest::Url = base
        .parse()
        .map_err(|e| anyhow!("bad server url {base:?}: {e}"))?;
    let trimmed = base.path().trim_start_matches('/').trim_end_matches('/');
    let candidates: Vec<String> = if trimmed.is_empty() {
        vec![
            "/.well-known/oauth-authorization-server".into(),
            "/.well-known/openid-configuration".into(),
        ]
    } else {
        vec![
            format!("/.well-known/oauth-authorization-server/{trimmed}"),
            format!("/.well-known/openid-configuration/{trimmed}"),
            format!("/{trimmed}/.well-known/openid-configuration"),
            "/.well-known/oauth-authorization-server".into(),
        ]
    };
    let client = reqwest::Client::new();
    for path in &candidates {
        let mut url = base.clone();
        url.set_query(None);
        url.set_fragment(None);
        url.set_path(path);
        let Ok(resp) = client.get(url).send().await else {
            continue;
        };
        if resp.status() != reqwest::StatusCode::OK {
            continue;
        }
        if let Ok(md) = resp.json::<AuthorizationMetadata>().await {
            return Ok(md);
        }
    }
    anyhow::bail!("oauth: no authorization server metadata found for {base}")
}

fn warn_relaxed_issuer(url: &str, md: &AuthorizationMetadata) {
    tracing::warn!(
        server_url = url,
        issuer = md.issuer.as_deref().unwrap_or("<missing>"),
        "oauth: issuer differs from the MCP server host; trusting discovered metadata (relaxed RFC 8414 issuer check)"
    );
}

/// ponytail: heuristic — rmcp surfaces a provider's invalid_client only as an
/// error string; the registration is gone server-side (e.g. non-durable DCR),
/// so drop client_id+tokens and let the next call re-register and re-authorize.
fn is_stale_client_error(e: &AuthError) -> bool {
    matches!(
        e,
        AuthError::TokenRefreshFailed(m) | AuthError::TokenRefreshRejected(m)
            if m.contains("invalid_client")
    )
}

/// OAuth state for one server: the rmcp state machine plus the pending
/// callback-listener task of an in-flight authorization.
pub struct OAuth {
    state: Mutex<Option<OAuthState>>,
    listener: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl OAuth {
    pub fn new() -> Arc<OAuth> {
        Arc::new(OAuth {
            state: Mutex::new(None),
            listener: Mutex::new(None),
        })
    }

    async fn ensure_state(&self, cfg: &ServerConfig, store: &TokenStore) -> anyhow::Result<()> {
        let mut g = self.state.lock().await;
        if g.is_none() {
            let url = cfg.url.as_deref().unwrap_or_default();
            let mut m = AuthorizationManager::new(url)
                .await
                .map_err(|e| anyhow!("oauth init for {url}: {e}"))?;
            m.set_credential_store(store.clone());
            if let Err(e) = m.initialize_from_store().await {
                if matches!(e, AuthError::AuthorizationServerMismatch { .. }) {
                    let md = discover_metadata_trusting_issuer(url).await?;
                    warn_relaxed_issuer(url, &md);
                    m.set_metadata(md);
                    m.initialize_from_store()
                        .await
                        .map_err(|e| anyhow!("oauth credential init: {e}"))?;
                } else {
                    return Err(anyhow!("oauth credential init: {e}"));
                }
            }
            *g = Some(OAuthState::Unauthorized(m));
        }
        Ok(())
    }

    /// Ok(Some) = valid token (refreshed + persisted by the manager if needed),
    /// Ok(None) = user authorization required, Err = infrastructure failure.
    pub async fn access_token(
        &self,
        cfg: &ServerConfig,
        store: &TokenStore,
    ) -> anyhow::Result<Option<String>> {
        self.ensure_state(cfg, store).await?;
        let g = self.state.lock().await;
        // rmcp quirk: OAuthState::get_access_token always errors "Already
        // authorized" once the state machine reaches Authorized — the token is
        // only reachable through the inner manager. An in-process browser flow
        // lands in Authorized; fresh processes read via Unauthorized(manager)
        // and never notice. Read the manager directly in that state.
        let res = match g.as_ref().unwrap() {
            OAuthState::Authorized(m) => m.get_access_token().await,
            st => st.get_access_token().await,
        };
        match res {
            Ok(t) => Ok(Some(t)),
            Err(AuthError::AuthorizationRequired) => Ok(None),
            Err(e) if is_stale_client_error(&e) => {
                tracing::warn!(%e, "oauth: stored client registration rejected; clearing credentials to re-register");
                store
                    .clear()
                    .await
                    .map_err(|e| anyhow!("oauth token: {e}"))?;
                Ok(None)
            }
            Err(e) => Err(anyhow!("oauth token: {e}")),
        }
    }

    /// Start (or re-report) the authorization flow; returns the URL to open.
    /// The background task waits for the browser redirect on 127.0.0.1 and
    /// completes the code exchange on its own.
    pub async fn begin_flow(
        self: &Arc<Self>,
        cfg: &ServerConfig,
        store: &TokenStore,
    ) -> anyhow::Result<String> {
        self.ensure_state(cfg, store).await?;
        let mut g = self.state.lock().await;
        {
            let st = g.as_mut().unwrap();
            if matches!(st, OAuthState::Session(_)) {
                return st.get_authorization_url().await.map_err(|e| anyhow!("{e}"));
            }
        }
        // bind first: the port is part of the redirect URI
        let port = cfg.oauth_redirect_port.unwrap_or(0);
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
            .await
            .map_err(|e| anyhow!("cannot bind oauth callback on 127.0.0.1:{port}: {e}"))?;
        let port = listener.local_addr()?.port();
        let client_id = match &cfg.oauth_client_id {
            Some(c) => Some(c.clone()),
            None => store.stored_client_id().await,
        };
        let build_req = || {
            let mut req = AuthorizationRequest::new(format!("http://127.0.0.1:{port}/callback"))
                .with_client_name("mcp-multiplexer");
            if !cfg.oauth_scopes.is_empty() {
                req = req.with_scopes(cfg.oauth_scopes.clone());
            }
            if let Some(cid) = &client_id {
                req = req.with_preregistered_client(cid.clone());
            }
            req
        };
        let start = g.as_mut().unwrap().start_authorization(build_req()).await;
        match start {
            Ok(()) => {}
            Err(AuthError::AuthorizationServerMismatch { .. }) => {
                // rmcp's strict discovery restored Unauthorized state; redo
                // discovery trustingly and build the session directly.
                let url = cfg.url.as_deref().unwrap_or_default();
                let md = discover_metadata_trusting_issuer(url).await?;
                warn_relaxed_issuer(url, &md);
                let taken = g.take().unwrap();
                let OAuthState::Unauthorized(mut m) = taken else {
                    *g = Some(taken);
                    bail!("oauth: unexpected state after failed discovery");
                };
                m.set_metadata(md);
                match AuthorizationSession::new(m, build_req()).await {
                    Ok(session) => *g = Some(OAuthState::Session(session)),
                    Err((m, e)) => {
                        *g = Some(OAuthState::Unauthorized(m));
                        return Err(anyhow!("oauth: {e}"));
                    }
                }
            }
            Err(e) => return Err(anyhow!("oauth: {e}")),
        }
        let url = g
            .as_mut()
            .unwrap()
            .get_authorization_url()
            .await
            .map_err(|e| anyhow!("{e}"))?;
        let me = self.clone();
        let task = tokio::spawn(async move {
            let Ok(callback_url) = wait_for_callback(listener).await else {
                tracing::debug!("oauth callback listener closed without a redirect");
                return;
            };
            let mut g = me.state.lock().await;
            if let Some(st) = g.as_mut() {
                match st.handle_callback_url(&callback_url).await {
                    Ok(()) => tracing::info!("oauth authorization completed"),
                    Err(e) => tracing::warn!(%e, "oauth callback exchange failed"),
                }
            }
        });
        *self.listener.lock().await = Some(task);
        Ok(url)
    }

    /// Headless completion: the user pastes the final redirect URL here.
    pub async fn complete_with_url(&self, pasted: &str) -> anyhow::Result<()> {
        let mut g = self.state.lock().await;
        let Some(st) = g.as_mut() else {
            bail!("no authorization in progress — call authorize_server without pasted_url first");
        };
        if !matches!(st, OAuthState::Session(_)) {
            bail!("no authorization in progress — call authorize_server without pasted_url first");
        }
        st.handle_callback_url(pasted)
            .await
            .map_err(|e| anyhow!("redirect URL rejected: {e} — if this persists, restart the flow via authorize_server"))
    }
}

/// Extract the request path from an HTTP request line ("GET /cb?a=b HTTP/1.1").
fn request_path(line: &str) -> Option<&str> {
    let mut parts = line.split_whitespace();
    match (parts.next(), parts.next()) {
        (Some("GET"), Some(p)) => Some(p),
        _ => None,
    }
}

/// Wait (max 10 min) for the browser to hit the callback, answer with a
/// "close this tab" page, and return the full redirect URL for parsing.
async fn wait_for_callback(listener: tokio::net::TcpListener) -> anyhow::Result<String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (mut sock, _) =
        tokio::time::timeout(std::time::Duration::from_secs(600), listener.accept())
            .await
            .map_err(|_| anyhow!("oauth callback timed out after 10 minutes"))??;
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    let line = loop {
        let n = sock.read(&mut chunk).await?;
        if n == 0 {
            bail!("connection closed before request");
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(end) = buf.windows(2).position(|w| w == b"\r\n") {
            break String::from_utf8_lossy(&buf[..end]).into_owned();
        }
        if buf.len() > 8192 {
            bail!("callback request too large");
        }
    };
    let path = request_path(&line).ok_or_else(|| anyhow!("not a GET request: {line:?}"))?;
    let body = "<html><body><h3>mcp-multiplexer</h3><p>Authorization complete - you can close this tab.</p></body></html>";
    sock.write_all(
        format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
            .as_bytes(),
    )
    .await?;
    Ok(format!("http://127.0.0.1{path}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_request_line() {
        assert_eq!(
            request_path("GET /callback?code=abc&state=xyz HTTP/1.1"),
            Some("/callback?code=abc&state=xyz")
        );
        assert_eq!(request_path("POST /callback HTTP/1.1"), None);
        assert_eq!(request_path("garbage"), None);
    }

    #[test]
    fn detects_stale_client_registration() {
        let e = AuthError::TokenRefreshFailed(
            "Server returned error response: invalid_client: Invalid client_id".into(),
        );
        assert!(is_stale_client_error(&e));
        assert!(!is_stale_client_error(&AuthError::AuthorizationRequired));
        assert!(!is_stale_client_error(&AuthError::TokenRefreshFailed(
            "connection refused".into()
        )));
    }

    #[tokio::test]
    async fn relaxed_discovery_trusts_cross_host_issuer() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let body = r#"{"issuer":"https://auth.other-host.example","authorization_endpoint":"https://auth.other-host.example/authorize","token_endpoint":"https://auth.other-host.example/token"}"#;
            while let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = [0u8; 2048];
                let n = sock.read(&mut buf).await.unwrap();
                let req = String::from_utf8_lossy(&buf[..n]);
                // only the canonical well-known path serves metadata; the
                // path-insertion candidates 404, exercising the fallback order
                let (status, body) =
                    if req.starts_with("GET /.well-known/oauth-authorization-server ") {
                        ("200 OK", body)
                    } else {
                        ("404 Not Found", "not found")
                    };
                sock.write_all(
                    format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes(),
                )
                .await
                .unwrap();
            }
        });
        let md = discover_metadata_trusting_issuer(&format!("http://127.0.0.1:{port}/mcp"))
            .await
            .unwrap();
        assert_eq!(
            md.issuer.as_deref(),
            Some("https://auth.other-host.example")
        );
        assert_eq!(md.token_endpoint, "https://auth.other-host.example/token");
        server.abort();
    }

    #[tokio::test]
    async fn token_store_roundtrip() {
        let dir = std::env::temp_dir().join(format!("mcpmux-oauth-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let lock = Arc::new(Mutex::new(()));
        let a = TokenStore {
            server: "a".into(),
            path: dir.join("tokens.json"),
            lock: lock.clone(),
        };
        let b = TokenStore {
            server: "b".into(),
            path: dir.join("tokens.json"),
            lock,
        };
        let creds = StoredCredentials::new("cid".into(), None, vec!["s1".into()], None);
        CredentialStore::save(&a, creds).await.unwrap();
        let got = CredentialStore::load(&a).await.unwrap().unwrap();
        assert_eq!(got.client_id, "cid");
        assert_eq!(got.granted_scopes, vec!["s1"]);
        assert!(
            CredentialStore::load(&b).await.unwrap().is_none(),
            "servers must not see each other's tokens"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(dir.join("tokens.json"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        CredentialStore::clear(&a).await.unwrap();
        assert!(CredentialStore::load(&a).await.unwrap().is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
