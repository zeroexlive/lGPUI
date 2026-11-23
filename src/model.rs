use serde::{Deserialize, Serialize};

/// A chat (group or 1:1) as returned by `GET /open-apis/im/v1/chats`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chat {
    pub chat_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub avatar: String,
    #[serde(default)]
    pub owner_id: String,
}

impl Chat {
    pub fn display_name(&self) -> &str {
        if self.name.trim().is_empty() { "(unnamed chat)" } else { &self.name }
    }

    /// One or two uppercase initials for the avatar placeholder.
    pub fn initials(&self) -> String {
        let initials: String = self
            .display_name()
            .split_whitespace()
            .filter_map(|w| w.chars().find(|c| c.is_alphanumeric()))
            .take(2)
            .flat_map(|c| c.to_uppercase())
            .collect();
        if initials.is_empty() { "#".into() } else { initials }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Sender {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub id_type: String,
    #[serde(default)]
    pub sender_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MessageBody {
    #[serde(default)]
    pub content: String,
}

/// A message as returned by `GET /open-apis/im/v1/messages`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub message_id: String,
    #[serde(default)]
    pub msg_type: String,
    /// Milliseconds since the Unix epoch, as a decimal string.
    #[serde(default)]
    pub create_time: String,
    #[serde(default)]
    pub chat_id: String,
    #[serde(default)]
    pub sender: Sender,
    #[serde(default)]
    pub body: MessageBody,
    #[serde(default)]
    pub deleted: bool,
}

impl Message {
    pub fn timestamp_ms(&self) -> i64 {
        self.create_time.parse().unwrap_or(0)
    }

    pub fn is_from_bot(&self) -> bool {
        self.sender.sender_type == "app"
    }

    /// Human-readable text for the message body.
    ///
    /// `text` messages carry `{"text": "..."}`; `post` (rich text) messages
    /// carry nested paragraphs of elements, which are flattened here.
    pub fn plain_text(&self) -> String {
        if self.deleted {
            return "[message recalled]".into();
        }
        let v: serde_json::Value = match serde_json::from_str(&self.body.content) {
            Ok(v) => v,
            Err(_) => return self.body.content.clone(),
        };
        match self.msg_type.as_str() {
            "text" => v.get("text").and_then(|t| t.as_str()).unwrap_or_default().to_string(),
            "post" => flatten_post(&v),
            "image" => "[image]".into(),
            "file" => format!(
                "[file] {}",
                v.get("file_name").and_then(|t| t.as_str()).unwrap_or_default()
            ),
            "sticker" => "[sticker]".into(),
            "audio" => "[voice message]".into(),
            "media" => "[video]".into(),
            "interactive" => "[card]".into(),
            "system" => "[system message]".into(),
            other => format!("[{other}]"),
        }
    }
}

fn flatten_post(v: &serde_json::Value) -> String {
    // A post may be `{title, content}` or keyed by locale: `{"en_us": {...}}`.
    let post = if v.get("content").is_some() {
        v
    } else {
        match v.as_object().and_then(|o| o.values().next()) {
            Some(inner) => inner,
            None => return String::new(),
        }
    };
    let mut out = Vec::new();
    if let Some(t) = post.get("title").and_then(|t| t.as_str()) {
        if !t.is_empty() {
            out.push(t.to_string());
        }
    }
    if let Some(paras) = post.get("content").and_then(|c| c.as_array()) {
        for para in paras {
            let line: String = para
                .as_array()
                .map(|els| {
                    els.iter()
                        .map(|el| match el.get("tag").and_then(|t| t.as_str()) {
                            Some("text") | Some("a") => {
                                el.get("text").and_then(|t| t.as_str()).unwrap_or("").to_string()
                            }
                            Some("at") => format!(
                                "@{}",
                                el.get("user_name").and_then(|t| t.as_str()).unwrap_or("user")
                            ),
                            Some("img") => "[image]".into(),
                            Some("emotion") => "[emoji]".into(),
                            _ => String::new(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            out.push(line);
        }
    }
    out.join("\n")
}

/// Format a millisecond timestamp as `HH:MM` (UTC) without extra dependencies.
pub fn format_hhmm(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let day_secs = secs.rem_euclid(86_400);
    format!("{:02}:{:02}", day_secs / 3600, (day_secs % 3600) / 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(msg_type: &str, content: &str) -> Message {
        Message {
            message_id: "om_1".into(),
            msg_type: msg_type.into(),
            create_time: "1700000000000".into(),
            chat_id: "oc_1".into(),
            sender: Sender::default(),
            body: MessageBody { content: content.into() },
            deleted: false,
        }
    }

    #[test]
    fn text_message() {
        assert_eq!(msg("text", r#"{"text":"hi there"}"#).plain_text(), "hi there");
    }

    #[test]
    fn post_message_flattens_paragraphs() {
        let c = r#"{"title":"T","content":[[{"tag":"text","text":"a "},{"tag":"at","user_name":"Bo"}],[{"tag":"img"}]]}"#;
        assert_eq!(msg("post", c).plain_text(), "T\na @Bo\n[image]");
    }

    #[test]
    fn localized_post() {
        let c = r#"{"en_us":{"title":"","content":[[{"tag":"text","text":"x"}]]}}"#;
        assert_eq!(msg("post", c).plain_text(), "x");
    }

    #[test]
    fn deleted_and_unknown() {
        let mut m = msg("text", r#"{"text":"x"}"#);
        m.deleted = true;
        assert_eq!(m.plain_text(), "[message recalled]");
        assert_eq!(msg("share_chat", "{}").plain_text(), "[share_chat]");
        assert_eq!(msg("text", "not json").plain_text(), "not json");
    }

    #[test]
    fn initials_and_time() {
        let c = Chat {
            chat_id: "1".into(),
            name: "design team".into(),
            description: String::new(),
            avatar: String::new(),
            owner_id: String::new(),
        };
        assert_eq!(c.initials(), "DT");
        assert_eq!(format_hhmm(1_700_000_000_000), "22:13");
    }
}
