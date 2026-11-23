//! UI-independent application state. The GPUI view is a thin renderer over this.

use std::collections::HashMap;

use crate::model::{Chat, Message};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Idle,
    Loading(String),
    Error(String),
}

#[derive(Debug, Default)]
pub struct Composer {
    text: String,
}

impl Composer {
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn is_blank(&self) -> bool {
        self.text.trim().is_empty()
    }
    pub fn insert(&mut self, s: &str) {
        // Ignore control characters other than newline.
        self.text.extend(s.chars().filter(|c| !c.is_control() || *c == '\n'));
    }
    pub fn backspace(&mut self) {
        self.text.pop();
    }
    /// Remove the trailing word (ctrl-backspace).
    pub fn delete_word(&mut self) {
        let trimmed = self.text.trim_end().len();
        self.text.truncate(trimmed);
        let cut = self.text.rfind(char::is_whitespace).map(|i| i + 1).unwrap_or(0);
        self.text.truncate(cut);
    }
    /// Returns the trimmed text and clears the composer, or `None` when blank.
    pub fn take(&mut self) -> Option<String> {
        if self.is_blank() {
            return None;
        }
        let t = self.text.trim().to_string();
        self.text.clear();
        Some(t)
    }
}

#[derive(Debug)]
pub struct Store {
    pub chats: Vec<Chat>,
    pub selected: Option<String>,
    pub messages: HashMap<String, Vec<Message>>,
    pub composer: Composer,
    pub filter: String,
    pub status: Status,
    pub offline: bool,
}

impl Store {
    pub fn new(offline: bool) -> Self {
        Self {
            chats: Vec::new(),
            selected: None,
            messages: HashMap::new(),
            composer: Composer::default(),
            filter: String::new(),
            status: Status::Idle,
            offline,
        }
    }

    pub fn set_chats(&mut self, chats: Vec<Chat>) {
        self.chats = chats;
        let still_there = self
            .selected
            .as_ref()
            .is_some_and(|id| self.chats.iter().any(|c| &c.chat_id == id));
        if !still_there {
            self.selected = self.chats.first().map(|c| c.chat_id.clone());
        }
    }

    /// Select a chat; returns true when the selection changed.
    pub fn select(&mut self, chat_id: &str) -> bool {
        if self.selected.as_deref() == Some(chat_id) {
            return false;
        }
        self.selected = Some(chat_id.to_string());
        true
    }

    pub fn selected_chat(&self) -> Option<&Chat> {
        let id = self.selected.as_ref()?;
        self.chats.iter().find(|c| &c.chat_id == id)
    }

    pub fn visible_chats(&self) -> Vec<&Chat> {
        let f = self.filter.to_lowercase();
        self.chats
            .iter()
            .filter(|c| f.is_empty() || c.display_name().to_lowercase().contains(&f))
            .collect()
    }

    pub fn set_messages(&mut self, chat_id: &str, mut msgs: Vec<Message>) {
        msgs.sort_by_key(|m| m.timestamp_ms());
        msgs.dedup_by(|a, b| a.message_id == b.message_id);
        self.messages.insert(chat_id.to_string(), msgs);
    }

    pub fn push_message(&mut self, msg: Message) {
        let list = self.messages.entry(msg.chat_id.clone()).or_default();
        if !list.iter().any(|m| m.message_id == msg.message_id) {
            list.push(msg);
            list.sort_by_key(|m| m.timestamp_ms());
        }
    }

    pub fn current_messages(&self) -> &[Message] {
        self.selected
            .as_ref()
            .and_then(|id| self.messages.get(id))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Text of the latest message in a chat, for the sidebar preview.
    pub fn preview(&self, chat_id: &str) -> Option<String> {
        self.messages
            .get(chat_id)
            .and_then(|m| m.last())
            .map(|m| m.plain_text().lines().next().unwrap_or("").to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo;

    #[test]
    fn composer_editing() {
        let mut c = Composer::default();
        c.insert("hello worx");
        c.backspace();
        c.insert("ld\u{7}");
        assert_eq!(c.text(), "hello world");
        c.delete_word();
        assert_eq!(c.text(), "hello ");
        assert_eq!(c.take().as_deref(), Some("hello"));
        assert!(c.take().is_none());
    }

    #[test]
    fn selection_and_filter() {
        let mut s = Store::new(true);
        s.set_chats(demo::chats());
        assert_eq!(s.selected.as_deref(), Some("oc_demo_general"));
        assert!(s.select("oc_demo_rust"));
        assert!(!s.select("oc_demo_rust"));
        s.filter = "DES".into();
        let v = s.visible_chats();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].chat_id, "oc_demo_design");
        // Reloading chats keeps a valid selection.
        s.set_chats(demo::chats());
        assert_eq!(s.selected.as_deref(), Some("oc_demo_rust"));
    }

    #[test]
    fn messages_are_sorted_and_deduped() {
        let mut s = Store::new(true);
        s.set_chats(demo::chats());
        let mut msgs = demo::messages("oc_demo_general");
        msgs.reverse();
        msgs.push(msgs[0].clone());
        s.set_messages("oc_demo_general", msgs);
        let cur = s.current_messages();
        assert_eq!(cur.len(), 2);
        assert!(cur[0].timestamp_ms() < cur[1].timestamp_ms());
        let m = demo::message("oc_demo_general", 9, true, cur[1].timestamp_ms() + 1, "new");
        s.push_message(m.clone());
        s.push_message(m);
        assert_eq!(s.current_messages().len(), 3);
        assert_eq!(s.preview("oc_demo_general").as_deref(), Some("new"));
    }
}
