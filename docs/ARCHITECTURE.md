# Architecture notes

## What the official Lark desktop client is made of

Findings from inspecting the public Linux package `Lark-linux_x64-8.0.18.deb`
(package `bytedance-lark-stable` 8.0.18, amd64, ~1.9 GB installed). Only file
layout, library names and public metadata were examined; no proprietary code
is included in, or was copied into, this repository.

| Layer | What ships | Notes |
|---|---|---|
| Shell | `lark` ELF + `libframe.so` (~345 MB), `*.pak`, `icudtl.dat`, SwiftShader/ANGLE | A Chromium-based desktop runtime (manifest cites a Chromium m147 branch), not stock Electron |
| Core SDK | `liblark.so` (~116 MB) | Rust: symbol paths reference crates/modules for `message`, `chat`, `feed`, `mail`, `calendar`, `search`, `settings`, `resource`, `passport`, `net`, `drive`, `thread`, `moments`… |
| IPC schema | `pb.desc`, `pb_server.desc` | Protobuf descriptors; ~3.8k request/response types, ~800 for IM (e.g. `CreateChat`, `CreateReaction`, `ClearChatMessages`, `BatchMuteFeedCards`) |
| UI | `webcontent/*.asar` (~767 MB) | One web bundle per product: `messenger`, `messenger-modals`, `main-window`, `calendar`, `mail`, `todo`, `space`/`docs`/`wiki`/`bitable`, `contacts`, `search`, `setting`, `video-conference-*`, `openplatform`, `miniprogram`… |
| Media / meetings | `libVolcEngineRTC.so`, `libbyteview-*.so`, `libRTCFFmpeg.so`, `RTCSDK_RES/` | Volcano Engine RTC stack, effects (background blur) and recording |
| Networking | `libsscronet.so` | Cronet-based HTTP stack |
| Misc | `libmonitor.so`, `liblark_crashpad.so`, `lib_lightgbm.so`, `libbytenn.so` | Telemetry, crash reporting, on-device ML |

So the official client is: **Rust core SDK ↔ protobuf IPC ↔ many web UIs inside
a custom Chromium shell.**

## How lGPUI maps onto that

lGPUI replaces the shell + web UI with a single native GPUI window and replaces
the private Rust SDK/protocol with Lark's **public Open Platform REST API**:

```
┌──────────── GPUI window (src/main.rs) ────────────┐
│ sidebar (chats, search) │ conversation + composer │
└──────────────┬────────────────────────────────────┘
               │ renders
        store::Store  (pure state, unit tested)
               │ background executor
        api::LarkClient  (ureq, tenant token cache)
               │ HTTPS
   open.larksuite.com / open.feishu.cn  /open-apis/...
```

| Official module | lGPUI equivalent | Status |
|---|---|---|
| messenger / feed | Chat list + conversation view | ✅ |
| message send | `POST im/v1/messages` (text) | ✅ |
| message types | text, post (rich text) flattened; image/file/card shown as placeholders | ✅ partial |
| realtime push | 5 s polling of the open chat | ✅ (event subscription is future work) |
| search | Local chat-name filter | ✅ |
| passport (user login) | Tenant (bot) token from app credentials | ✅ (user OAuth is future work) |
| calendar, mail, docs, meetings | – | not in scope |

## Why the public API

The private protocol is undocumented, changes every release and its terms
forbid reuse; the Open Platform API is documented, versioned and meant for
third-party clients. The trade-off is that lGPUI acts as a **bot** (it sees the
chats your app's bot has been added to, and messages are sent as the bot).
