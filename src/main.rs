//! lGPUI: native Lark messenger window built with GPUI.

use std::sync::Arc;
use std::time::Duration;

use gpui::{
    App, Application, Bounds, Context, FocusHandle, Hsla, KeyDownEvent, MouseButton, SharedString,
    Window, WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};

use lgpui::api::Backend;
use lgpui::demo;
use lgpui::model::{Chat, Message, format_hhmm};
use lgpui::store::{Status, Store};

const POLL_INTERVAL: Duration = Duration::from_secs(5);
const MESSAGE_LIMIT: usize = 50;

mod theme {
    pub const SIDEBAR_BG: u32 = 0x1f2329;
    pub const SIDEBAR_HOVER: u32 = 0x2b3038;
    pub const SIDEBAR_ACTIVE: u32 = 0x3370ff;
    pub const SIDEBAR_TEXT: u32 = 0xe8eaed;
    pub const SIDEBAR_MUTED: u32 = 0x8f959e;
    pub const MAIN_BG: u32 = 0xffffff;
    pub const HEADER_BORDER: u32 = 0xdee0e3;
    pub const TEXT: u32 = 0x1f2329;
    pub const MUTED: u32 = 0x8f959e;
    pub const BUBBLE_ME: u32 = 0xcce0ff;
    pub const BUBBLE_OTHER: u32 = 0xf2f3f5;
    pub const ACCENT: u32 = 0x3370ff;
    pub const ERROR: u32 = 0xf54a45;
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InputTarget {
    Composer,
    Search,
}

struct LarkApp {
    store: Store,
    client: Option<Arc<Backend>>,
    focus: FocusHandle,
    target: InputTarget,
}

impl LarkApp {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let client = Backend::detect().map(Arc::new);
        let focus = cx.focus_handle();
        window.focus(&focus);
        let mut this = Self {
            store: Store::new(client.is_none()),
            client,
            focus,
            target: InputTarget::Composer,
        };
        this.load_chats(cx);
        this.start_polling(cx);
        this
    }

    fn load_chats(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            self.store.set_chats(demo::chats());
            for c in demo::chats() {
                self.store.set_messages(&c.chat_id, demo::messages(&c.chat_id));
            }
            return;
        };
        self.store.status = Status::Loading("Loading chats…".into());
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { client.list_chats(10) }).await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(chats) => {
                        this.store.status = Status::Idle;
                        this.store.set_chats(chats);
                        this.load_messages(cx);
                    }
                    Err(e) => this.store.status = Status::Error(e.to_string()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn load_messages(&mut self, cx: &mut Context<Self>) {
        let (Some(client), Some(chat_id)) = (self.client.clone(), self.store.selected.clone()) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let id = chat_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move { client.list_messages(&id, MESSAGE_LIMIT) })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(msgs) => {
                        this.store.set_messages(&chat_id, msgs);
                        if matches!(this.store.status, Status::Error(_)) {
                            this.store.status = Status::Idle;
                        }
                    }
                    Err(e) => this.store.status = Status::Error(e.to_string()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn start_polling(&self, cx: &mut Context<Self>) {
        if self.client.is_none() {
            return;
        }
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL_INTERVAL).await;
                if this.update(cx, |this, cx| this.load_messages(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn select_chat(&mut self, chat_id: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.store.select(&chat_id) {
            self.load_messages(cx);
        }
        self.target = InputTarget::Composer;
        window.focus(&self.focus);
        cx.notify();
    }

    fn send(&mut self, cx: &mut Context<Self>) {
        let Some(chat_id) = self.store.selected.clone() else { return };
        let Some(text) = self.store.composer.take() else { return };
        let Some(client) = self.client.clone() else {
            // Offline demo: echo locally.
            let n = self.store.current_messages().len();
            let ts = self.store.current_messages().last().map(|m| m.timestamp_ms() + 60_000).unwrap_or(0);
            self.store.push_message(demo::message(&chat_id, n + 100, true, ts, &text));
            cx.notify();
            return;
        };
        self.store.status = Status::Loading("Sending…".into());
        cx.spawn(async move |this, cx| {
            let id = chat_id.clone();
            let result = cx.background_executor().spawn(async move { client.send_text(&id, &text) }).await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(mut msg) => {
                        if msg.chat_id.is_empty() {
                            msg.chat_id = chat_id;
                        }
                        this.store.push_message(msg);
                        this.store.status = Status::Idle;
                    }
                    Err(e) => this.store.status = Status::Error(format!("Send failed: {e}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn on_key(&mut self, ev: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        let m = ks.modifiers;
        let command = m.control || m.platform;
        match (self.target, ks.key.as_str()) {
            (InputTarget::Composer, "enter") if m.shift => self.store.composer.insert("\n"),
            (InputTarget::Composer, "enter") => self.send(cx),
            (InputTarget::Composer, "backspace") if command || m.alt => self.store.composer.delete_word(),
            (InputTarget::Composer, "backspace") => self.store.composer.backspace(),
            (InputTarget::Search, "backspace") => {
                self.store.filter.pop();
            }
            (InputTarget::Search, "escape") | (InputTarget::Search, "enter") => {
                if ks.key == "escape" {
                    self.store.filter.clear();
                }
                self.target = InputTarget::Composer;
            }
            (_, "v") if command => {
                if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                    self.insert_text(&text);
                }
            }
            (_, "r") if command => self.load_chats(cx),
            _ if !command => {
                if let Some(ch) = ks.key_char.as_deref() {
                    let ch = ch.to_string();
                    self.insert_text(&ch);
                }
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn insert_text(&mut self, s: &str) {
        match self.target {
            InputTarget::Composer => self.store.composer.insert(s),
            InputTarget::Search => self.store.filter.extend(s.chars().filter(|c| !c.is_control())),
        }
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let searching = self.target == InputTarget::Search;
        let filter = self.store.filter.clone();
        let search_text: SharedString = if filter.is_empty() && !searching {
            "Search chats".into()
        } else {
            format!("{filter}{}", if searching { "▏" } else { "" }).into()
        };
        let rows = self
            .store
            .visible_chats()
            .into_iter()
            .cloned()
            .collect::<Vec<Chat>>()
            .into_iter()
            .map(|chat| self.render_chat_row(chat, cx))
            .collect::<Vec<_>>();

        div()
            .flex()
            .flex_col()
            .w(px(280.))
            .h_full()
            .bg(rgb(theme::SIDEBAR_BG))
            .text_color(rgb(theme::SIDEBAR_TEXT))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_4()
                    .py_3()
                    .child(div().text_lg().font_weight(gpui::FontWeight::BOLD).child("Messenger"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(theme::SIDEBAR_MUTED))
                            .child(self.client.as_ref().map_or("demo", |c| c.label())),
                    ),
            )
            .child(
                div()
                    .id("search")
                    .mx_3()
                    .mb_2()
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .bg(rgb(theme::SIDEBAR_HOVER))
                    .border_1()
                    .border_color(if searching { rgb(theme::ACCENT) } else { rgb(theme::SIDEBAR_HOVER) })
                    .text_sm()
                    .text_color(if filter.is_empty() && !searching {
                        rgb(theme::SIDEBAR_MUTED)
                    } else {
                        rgb(theme::SIDEBAR_TEXT)
                    })
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.target = InputTarget::Search;
                            window.focus(&this.focus);
                            cx.notify();
                        }),
                    )
                    .child(search_text),
            )
            .child(div().id("chat-list").flex().flex_col().flex_1().overflow_y_scroll().children(rows))
    }

    fn render_chat_row(&self, chat: Chat, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let selected = self.store.selected.as_deref() == Some(chat.chat_id.as_str());
        let preview = self
            .store
            .preview(&chat.chat_id)
            .unwrap_or_else(|| chat.description.clone());
        let id = chat.chat_id.clone();
        div()
            .id(SharedString::from(format!("chat-{}", chat.chat_id)))
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .mx_2()
            .rounded_md()
            .cursor_pointer()
            .when(selected, |d| d.bg(rgb(theme::SIDEBAR_ACTIVE)))
            .when(!selected, |d| d.hover(|s| s.bg(rgb(theme::SIDEBAR_HOVER))))
            .on_click(cx.listener(move |this, _, window, cx| this.select_chat(id.clone(), window, cx)))
            .child(avatar(&chat.initials(), 36.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .child(div().text_sm().font_weight(gpui::FontWeight::SEMIBOLD).child(chat.display_name().to_string()))
                    .child(
                        div()
                            .text_xs()
                            .text_color(if selected { rgb(theme::SIDEBAR_TEXT) } else { rgb(theme::SIDEBAR_MUTED) })
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(preview),
                    ),
            )
    }

    fn render_conversation(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(chat) = self.store.selected_chat().cloned() else {
            return div()
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .text_color(rgb(theme::MUTED))
                .child(match &self.store.status {
                    Status::Error(e) => e.clone(),
                    Status::Loading(s) => s.clone(),
                    Status::Idle => "No chats yet. Add your bot to a group in Lark.".into(),
                });
        };
        let messages: Vec<Message> = self.store.current_messages().to_vec();
        let composing = self.target == InputTarget::Composer;
        let draft = self.store.composer.text().to_string();
        let placeholder = draft.is_empty();

        div()
            .flex()
            .flex_col()
            .flex_1()
            .h_full()
            .bg(rgb(theme::MAIN_BG))
            .text_color(rgb(theme::TEXT))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .px_5()
                    .py_3()
                    .border_b_1()
                    .border_color(rgb(theme::HEADER_BORDER))
                    .child(div().text_base().font_weight(gpui::FontWeight::BOLD).child(chat.display_name().to_string()))
                    .when(!chat.description.is_empty(), |d| {
                        d.child(div().text_xs().text_color(rgb(theme::MUTED)).child(chat.description.clone()))
                    }),
            )
            .child(
                div()
                    .id("messages")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .gap_3()
                    .px_5()
                    .py_4()
                    .overflow_y_scroll()
                    .children(messages.into_iter().map(render_message)),
            )
            .child(self.render_status())
            .child(
                div()
                    .id("composer")
                    .m_4()
                    .px_3()
                    .py_2()
                    .min_h(px(44.))
                    .rounded_lg()
                    .border_1()
                    .border_color(if composing { rgb(theme::ACCENT) } else { rgb(theme::HEADER_BORDER) })
                    .text_sm()
                    .cursor_text()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.target = InputTarget::Composer;
                            window.focus(&this.focus);
                            cx.notify();
                        }),
                    )
                    .text_color(if placeholder { rgb(theme::MUTED) } else { rgb(theme::TEXT) })
                    .child(if placeholder {
                        format!("Message {}  (Enter to send, Shift+Enter for newline)", chat.display_name())
                    } else {
                        format!("{draft}{}", if composing { "▏" } else { "" })
                    }),
            )
    }

    fn render_status(&self) -> impl IntoElement {
        let (text, color): (String, Hsla) = match &self.store.status {
            Status::Idle => (String::new(), rgb(theme::MUTED).into()),
            Status::Loading(s) => (s.clone(), rgb(theme::MUTED).into()),
            Status::Error(e) => (e.clone(), rgb(theme::ERROR).into()),
        };
        div().px_5().text_xs().text_color(color).child(text)
    }
}

fn avatar(initials: &str, size_px: f32) -> impl IntoElement {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(size_px))
        .rounded_full()
        .bg(rgb(theme::ACCENT))
        .text_color(rgb(0xffffff))
        .text_xs()
        .font_weight(gpui::FontWeight::BOLD)
        .child(initials.to_string())
}

fn render_message(msg: Message) -> impl IntoElement {
    let mine = msg.is_from_bot();
    let sender = if mine { "You (bot)".to_string() } else { short_id(&msg.sender.id) };
    div()
        .flex()
        .flex_col()
        .when(mine, |d| d.items_end())
        .when(!mine, |d| d.items_start())
        .child(
            div()
                .flex()
                .gap_2()
                .text_xs()
                .text_color(rgb(theme::MUTED))
                .child(sender)
                .child(format_hhmm(msg.timestamp_ms())),
        )
        .child(
            div()
                .max_w(px(520.))
                .px_3()
                .py_2()
                .rounded_lg()
                .bg(rgb(if mine { theme::BUBBLE_ME } else { theme::BUBBLE_OTHER }))
                .text_sm()
                .child(msg.plain_text()),
        )
}

fn short_id(id: &str) -> String {
    if id.is_empty() {
        return "Unknown".into();
    }
    let tail: String = id.chars().rev().take(6).collect::<Vec<_>>().into_iter().rev().collect();
    format!("User …{tail}")
}

impl Render for LarkApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .flex()
            .size_full()
            .font_family(".SystemUIFont")
            .child(self.render_sidebar(cx))
            .child(self.render_conversation(cx))
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1100.), px(720.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("lGPUI — Lark".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| LarkApp::new(window, cx)),
        )
        .expect("failed to open window");
        cx.activate(true);
    });
}
