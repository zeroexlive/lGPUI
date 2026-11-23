# lGPUI

A fast, native **Lark / Feishu messenger** for the desktop, written from
scratch in **Rust** with [GPUI](https://crates.io/crates/gpui) (the UI framework
behind the Zed editor).

The official client is a ~2 GB Chromium shell hosting dozens of web apps;
lGPUI is a single native binary that talks to Lark's public Open Platform API.
See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for how the two compare.

## Features

- Chat list with last-message previews and instant search
- Conversation view with text and rich-text (post) messages, timestamps, bubbles
- Composer: Enter to send, Shift+Enter for a newline, Ctrl+Backspace deletes a word, Ctrl+V pastes
- Auto-refresh of the open chat every 5 s, Ctrl+R reloads chats
- Works with Lark (larksuite.com) or Feishu (feishu.cn)
- Offline demo mode when no credentials are set

## Setup

1. Create a **custom app** at <https://open.larksuite.com/app> (or open.feishu.cn), enable the **Bot** feature.
2. Grant the scopes `im:chat:readonly`, `im:message`, `im:message:send_as_bot`, and
   `im:message.group_msg` (read group messages), then publish the version.
3. Add the bot to the group chats you want to see.
4. Run:

```bash
export LARK_APP_ID=cli_xxx
export LARK_APP_SECRET=xxxx
export LARK_DOMAIN=lark      # or: feishu
cargo run --release
```

Without the two credentials, lGPUI starts in demo mode with sample chats.

### Linux build dependencies

```bash
sudo apt install pkg-config libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev \
  libvulkan-dev libx11-xcb-dev libxcb1-dev libfontconfig-dev libfreetype-dev \
  libssl-dev libzstd-dev libasound2-dev cmake clang
```

macOS needs Xcode command-line tools; Windows is supported by GPUI as well.

## Development

```bash
cargo test --no-default-features   # core logic + API tests against a mock server, no GPU needed
cargo build                        # full app
```

Code layout:

| Path | Purpose |
|---|---|
| `src/api.rs` | Open Platform client: tenant token cache, chats, messages, send |
| `src/model.rs` | Chat / message types, rich-text flattening |
| `src/store.rs` | UI-independent state (selection, filter, composer) |
| `src/main.rs` | GPUI window, sidebar, conversation, keyboard handling |
| `tests/api_mock.rs` | HTTP-level tests with `mockito` |

## Roadmap

- Event subscription (WebSocket long connection) instead of polling
- User login (OAuth) so messages are sent as you, not the bot
- Images, files and interactive cards
- Threads, reactions, read receipts

## Disclaimer

Independent project, not affiliated with or endorsed by ByteDance, Lark or
Feishu. It contains no code from the official client.

## License

MIT
