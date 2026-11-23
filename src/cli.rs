//! Backend that drives the official `lark-cli` (github.com/larksuite/cli).
//!
//! `lark-cli` creates the Open Platform app for you (`lark-cli config init --new`)
//! and signs you in as yourself with OAuth (`lark-cli auth login`), keeping the
//! tokens in the OS keychain. lGPUI shells out to `lark-cli api ... --as user`
//! so it sees your own chats and sends messages as you, with no app secrets in
//! lGPUI at all.

use std::process::Command;

use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::api::{ApiError, Page, Result};
use crate::model::{Chat, Message};

pub struct CliClient {
    bin: String,
    identity: String,
}

impl CliClient {
    /// `LARK_CLI` overrides the binary (default `lark-cli` on PATH);
    /// `LARK_CLI_AS` picks `user` (default) or `bot`.
    pub fn from_env() -> Self {
        Self {
            bin: std::env::var("LARK_CLI").unwrap_or_else(|_| "lark-cli".into()),
            identity: std::env::var("LARK_CLI_AS").unwrap_or_else(|_| "user".into()),
        }
    }

    pub fn with_bin(bin: impl Into<String>) -> Self {
        Self { bin: bin.into(), identity: "user".into() }
    }

    /// True when `lark-cli` is installed and has a signed-in identity.
    pub fn is_ready(&self) -> bool {
        Command::new(&self.bin)
            .args(["auth", "status"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn call<T: DeserializeOwned>(&self, method: &str, path: &str, params: Value, data: Option<Value>) -> Result<T> {
        let mut cmd = Command::new(&self.bin);
        cmd.args(["api", method, path, "--as", &self.identity, "--format", "json"]);
        cmd.args(["--params", &params.to_string()]);
        if let Some(d) = data {
            cmd.args(["--data", &d.to_string()]);
        }
        let out = cmd
            .output()
            .map_err(|e| ApiError::Network(format!("cannot run {}: {e}", self.bin)))?;
        let text = if out.stdout.is_empty() { &out.stderr } else { &out.stdout };
        let v: Value = serde_json::from_slice(text).map_err(|e| {
            ApiError::Decode(format!("{e}: {}", String::from_utf8_lossy(text).chars().take(200).collect::<String>()))
        })?;
        let data = normalize(v)?;
        serde_json::from_value(data).map_err(|e| ApiError::Decode(e.to_string()))
    }

    pub fn list_chats(&self, max_pages: usize) -> Result<Vec<Chat>> {
        let mut out = Vec::new();
        let mut token = String::new();
        for _ in 0..max_pages.max(1) {
            let mut p = json!({ "page_size": 50 });
            if !token.is_empty() {
                p["page_token"] = json!(token);
            }
            let page: Page<Chat> = self.call("GET", "/open-apis/im/v1/chats", p, None)?;
            out.extend(page.items);
            match page.page_token.filter(|t| page.has_more && !t.is_empty()) {
                Some(t) => token = t,
                None => break,
            }
        }
        Ok(out)
    }

    pub fn list_messages(&self, chat_id: &str, limit: usize) -> Result<Vec<Message>> {
        let p = json!({
            "container_id_type": "chat",
            "container_id": chat_id,
            "sort_type": "ByCreateTimeDesc",
            "page_size": limit.clamp(1, 50),
        });
        let page: Page<Message> = self.call("GET", "/open-apis/im/v1/messages", p, None)?;
        let mut items = page.items;
        items.sort_by_key(|m| m.timestamp_ms());
        Ok(items)
    }

    pub fn send_text(&self, chat_id: &str, text: &str) -> Result<Message> {
        let body = json!({
            "receive_id": chat_id,
            "msg_type": "text",
            "content": json!({ "text": text }).to_string(),
        });
        self.call("POST", "/open-apis/im/v1/messages", json!({ "receive_id_type": "chat_id" }), Some(body))
    }
}

/// Unwrap lark-cli's `{ok, data | error}` envelope and/or the raw Open
/// Platform `{code, msg, data}` envelope, returning the inner `data`.
pub fn normalize(mut v: Value) -> Result<Value> {
    for _ in 0..3 {
        if let Some(ok) = v.get("ok").and_then(Value::as_bool) {
            if !ok {
                let e = v.get("error").cloned().unwrap_or(Value::Null);
                let code = e.get("code").and_then(Value::as_i64).unwrap_or(-1);
                let mut msg = e.get("message").and_then(Value::as_str).unwrap_or("lark-cli error").to_string();
                if let Some(h) = e.get("hint").and_then(Value::as_str) {
                    msg = format!("{msg} ({h})");
                }
                return Err(ApiError::Lark { code, msg });
            }
            v = v.get_mut("data").map(Value::take).unwrap_or(Value::Null);
            continue;
        }
        if let Some(code) = v.get("code").and_then(Value::as_i64) {
            if code != 0 {
                let msg = v.get("msg").and_then(Value::as_str).unwrap_or_default().to_string();
                return Err(ApiError::Lark { code, msg });
            }
            v = v.get_mut("data").map(Value::take).unwrap_or(Value::Null);
            continue;
        }
        break;
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unwraps_cli_and_openapi_envelopes() {
        let v = json!({"ok": true, "identity": "user", "data": {"code": 0, "msg": "ok", "data": {"items": [1]}}});
        assert_eq!(normalize(v).unwrap(), json!({"items": [1]}));
        let v = json!({"ok": true, "data": {"items": [2]}});
        assert_eq!(normalize(v).unwrap(), json!({"items": [2]}));
    }

    #[test]
    fn surfaces_errors() {
        let v = json!({"ok": false, "error": {"code": 99991679, "message": "no scope", "hint": "run auth login"}});
        match normalize(v) {
            Err(ApiError::Lark { code, msg }) => {
                assert_eq!(code, 99991679);
                assert!(msg.contains("run auth login"));
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(normalize(json!({"code": 230002, "msg": "bot not in chat"})), Err(ApiError::Lark { code: 230002, .. })));
    }

    #[cfg(unix)]
    #[test]
    fn drives_a_fake_cli() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("lgpui-cli-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bin = dir.join("lark-cli");
        std::fs::write(
            &bin,
            "#!/bin/sh\necho \"$@\" > \"$(dirname \"$0\")/args\"\necho '{\"ok\":true,\"identity\":\"user\",\"data\":{\"code\":0,\"data\":{\"items\":[{\"chat_id\":\"oc_1\",\"name\":\"Team\"}],\"has_more\":false}}}'\n",
        )
        .unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let c = CliClient::with_bin(bin.to_string_lossy());
        let chats = c.list_chats(3).unwrap();
        assert_eq!(chats.len(), 1);
        assert_eq!(chats[0].chat_id, "oc_1");
        let args = std::fs::read_to_string(dir.join("args")).unwrap();
        assert!(args.starts_with("api GET /open-apis/im/v1/chats --as user"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
