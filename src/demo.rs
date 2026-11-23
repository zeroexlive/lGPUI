//! Offline sample data so the UI is usable without Lark credentials.

use crate::model::{Chat, Message, MessageBody, Sender};

fn chat(id: &str, name: &str, desc: &str) -> Chat {
    Chat {
        chat_id: id.into(),
        name: name.into(),
        description: desc.into(),
        avatar: String::new(),
        owner_id: String::new(),
    }
}

pub fn chats() -> Vec<Chat> {
    vec![
        chat("oc_demo_general", "General", "Company-wide announcements"),
        chat("oc_demo_design", "Design Team", "Mockups and reviews"),
        chat("oc_demo_rust", "Rust Guild", "Ownership, borrowing and GPUI"),
    ]
}

pub fn message(chat_id: &str, id: usize, from_me: bool, ts_ms: i64, text: &str) -> Message {
    Message {
        message_id: format!("om_demo_{chat_id}_{id}"),
        msg_type: "text".into(),
        create_time: ts_ms.to_string(),
        chat_id: chat_id.into(),
        sender: Sender {
            id: if from_me { "cli_demo".into() } else { format!("ou_demo_{}", id % 3) },
            id_type: if from_me { "app_id".into() } else { "open_id".into() },
            sender_type: if from_me { "app".into() } else { "user".into() },
        },
        body: MessageBody { content: serde_json::json!({ "text": text }).to_string() },
        deleted: false,
    }
}

pub fn messages(chat_id: &str) -> Vec<Message> {
    let base = 1_763_890_000_000_i64;
    let lines: &[(bool, &str)] = match chat_id {
        "oc_demo_design" => &[
            (false, "New sidebar mockups are up."),
            (true, "Looks great. Can we try a narrower chat list?"),
            (false, "Sure, 280px it is."),
        ],
        "oc_demo_rust" => &[
            (false, "Anyone tried GPUI 0.2 on Linux?"),
            (true, "Yes, this client runs on it."),
        ],
        _ => &[
            (false, "Welcome to lGPUI, a native Lark client."),
            (false, "Set LARK_APP_ID and LARK_APP_SECRET to connect to your tenant."),
        ],
    };
    lines
        .iter()
        .enumerate()
        .map(|(i, (me, t))| message(chat_id, i, *me, base + i as i64 * 60_000, t))
        .collect()
}
