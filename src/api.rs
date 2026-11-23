//! Minimal, blocking client for the public Lark / Feishu Open Platform IM API.
//!
//! Endpoints used (all documented at open.larksuite.com/document):
//! * `POST /open-apis/auth/v3/tenant_access_token/internal`
//! * `GET  /open-apis/im/v1/chats`
//! * `GET  /open-apis/im/v1/messages`
//! * `POST /open-apis/im/v1/messages?receive_id_type=chat_id`
//!
//! The client is `Send + Sync` and is meant to be called from GPUI's
//! background executor.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::json;

use crate::config::Config;
use crate::model::{Chat, Message};

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("network error: {0}")]
    Network(String),
    #[error("Lark API error {code}: {msg}")]
    Lark { code: i64, msg: String },
    #[error("unexpected response: {0}")]
    Decode(String),
}

pub type Result<T> = std::result::Result<T, ApiError>;

#[derive(Deserialize)]
struct Envelope<T> {
    code: i64,
    #[serde(default)]
    msg: String,
    data: Option<T>,
}

#[derive(Deserialize)]
struct TokenResponse {
    code: i64,
    #[serde(default)]
    msg: String,
    #[serde(default)]
    tenant_access_token: String,
    #[serde(default)]
    expire: u64,
}

#[derive(Debug, Deserialize)]
pub struct Page<T> {
    #[serde(default = "Vec::new")]
    pub items: Vec<T>,
    #[serde(default)]
    pub has_more: bool,
    #[serde(default)]
    pub page_token: Option<String>,
}

struct CachedToken {
    value: String,
    refresh_at: Instant,
}

pub struct LarkClient {
    config: Config,
    agent: ureq::Agent,
    token: Mutex<Option<CachedToken>>,
}

impl LarkClient {
    pub fn new(config: Config) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(20))
            .user_agent(concat!("lgpui/", env!("CARGO_PKG_VERSION")))
            .build();
        Self { config, agent, token: Mutex::new(None) }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.config.base_url, path)
    }

    /// Returns a valid tenant access token, refreshing it five minutes before expiry.
    pub fn tenant_token(&self) -> Result<String> {
        let mut guard = self.token.lock().expect("token mutex poisoned");
        if let Some(t) = guard.as_ref() {
            if Instant::now() < t.refresh_at {
                return Ok(t.value.clone());
            }
        }
        let resp: TokenResponse = read_json(
            self.agent
                .post(&self.url("/open-apis/auth/v3/tenant_access_token/internal"))
                .send_json(json!({
                    "app_id": self.config.app_id,
                    "app_secret": self.config.app_secret,
                })),
        )?;
        if resp.code != 0 {
            return Err(ApiError::Lark { code: resp.code, msg: resp.msg });
        }
        let ttl = resp.expire.saturating_sub(300).max(60);
        *guard = Some(CachedToken {
            value: resp.tenant_access_token.clone(),
            refresh_at: Instant::now() + Duration::from_secs(ttl),
        });
        Ok(resp.tenant_access_token)
    }

    fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, &str)]) -> Result<T> {
        let token = self.tenant_token()?;
        let mut req = self.agent.get(&self.url(path)).set("Authorization", &format!("Bearer {token}"));
        for (k, v) in query {
            req = req.query(k, v);
        }
        unwrap_envelope(read_json(req.call())?)
    }

    /// One page of chats the app (bot) is a member of.
    pub fn list_chats_page(&self, page_token: Option<&str>) -> Result<Page<Chat>> {
        let mut q = vec![("page_size", "50")];
        if let Some(t) = page_token {
            q.push(("page_token", t));
        }
        self.get("/open-apis/im/v1/chats", &q)
    }

    /// All chats, following `page_token` up to `max_pages` pages.
    pub fn list_chats(&self, max_pages: usize) -> Result<Vec<Chat>> {
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        for _ in 0..max_pages.max(1) {
            let page = self.list_chats_page(token.as_deref())?;
            out.extend(page.items);
            match page.page_token.filter(|t| page.has_more && !t.is_empty()) {
                Some(t) => token = Some(t),
                None => break,
            }
        }
        Ok(out)
    }

    /// The newest `limit` messages of a chat, returned oldest first.
    pub fn list_messages(&self, chat_id: &str, limit: usize) -> Result<Vec<Message>> {
        let size = limit.clamp(1, 50).to_string();
        let page: Page<Message> = self.get(
            "/open-apis/im/v1/messages",
            &[
                ("container_id_type", "chat"),
                ("container_id", chat_id),
                ("sort_type", "ByCreateTimeDesc"),
                ("page_size", &size),
            ],
        )?;
        let mut items = page.items;
        items.sort_by_key(|m| m.timestamp_ms());
        Ok(items)
    }

    /// Send a plain-text message to a chat as the app's bot.
    pub fn send_text(&self, chat_id: &str, text: &str) -> Result<Message> {
        let token = self.tenant_token()?;
        let content = json!({ "text": text }).to_string();
        let resp = self
            .agent
            .post(&self.url("/open-apis/im/v1/messages"))
            .query("receive_id_type", "chat_id")
            .set("Authorization", &format!("Bearer {token}"))
            .send_json(json!({
                "receive_id": chat_id,
                "msg_type": "text",
                "content": content,
            }));
        unwrap_envelope(read_json(resp)?)
    }
}

fn read_json<T: DeserializeOwned>(resp: std::result::Result<ureq::Response, ureq::Error>) -> Result<T> {
    let resp = match resp {
        Ok(r) => r,
        // Lark returns JSON error bodies with HTTP 4xx; decode them for a useful message.
        Err(ureq::Error::Status(_, r)) => r,
        Err(e) => return Err(ApiError::Network(e.to_string())),
    };
    resp.into_json::<T>().map_err(|e| ApiError::Decode(e.to_string()))
}

fn unwrap_envelope<T>(env: Envelope<T>) -> Result<T> {
    if env.code != 0 {
        return Err(ApiError::Lark { code: env.code, msg: env.msg });
    }
    env.data.ok_or_else(|| ApiError::Decode("missing `data`".into()))
}

/// Where lGPUI gets its data from.
pub enum Backend {
    /// Direct REST calls with a custom app's ID/secret (acts as the bot).
    Rest(LarkClient),
    /// The official `lark-cli`, signed in as you (see `crate::cli`).
    Cli(crate::cli::CliClient),
}

impl Backend {
    /// `LARK_APP_ID`/`LARK_APP_SECRET` win; otherwise a signed-in `lark-cli`;
    /// otherwise `None` (demo mode).
    pub fn detect() -> Option<Self> {
        if let Some(cfg) = Config::from_env() {
            return Some(Backend::Rest(LarkClient::new(cfg)));
        }
        let cli = crate::cli::CliClient::from_env();
        cli.is_ready().then_some(Backend::Cli(cli))
    }

    pub fn label(&self) -> &'static str {
        match self {
            Backend::Rest(_) => "bot",
            Backend::Cli(_) => "lark-cli",
        }
    }

    pub fn list_chats(&self, max_pages: usize) -> Result<Vec<Chat>> {
        match self {
            Backend::Rest(c) => c.list_chats(max_pages),
            Backend::Cli(c) => c.list_chats(max_pages),
        }
    }

    pub fn list_messages(&self, chat_id: &str, limit: usize) -> Result<Vec<Message>> {
        match self {
            Backend::Rest(c) => c.list_messages(chat_id, limit),
            Backend::Cli(c) => c.list_messages(chat_id, limit),
        }
    }

    pub fn send_text(&self, chat_id: &str, text: &str) -> Result<Message> {
        match self {
            Backend::Rest(c) => c.send_text(chat_id, text),
            Backend::Cli(c) => c.send_text(chat_id, text),
        }
    }
}
