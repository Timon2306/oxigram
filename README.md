<div align="center">

<img src="native/icons/icon.png" width="120" alt="Oxigram logo">

# Oxigram

**A native Telegram desktop client in Rust that runs in a few dozen MB of RAM.**<br>
Rust talks MTProto directly and draws its own window. No TDLib, no Electron, no WebView, no Qt.

[![License: MIT](https://img.shields.io/badge/license-MIT-2a8cf0?style=flat-square)](LICENSE) [![Rust](https://img.shields.io/badge/Rust-grammers-dea584?style=flat-square&logo=rust&logoColor=white)](https://codeberg.org/Lonami/grammers) [![Slint](https://img.shields.io/badge/UI-Slint-2379f4?style=flat-square)](https://slint.dev) [![Platforms](https://img.shields.io/badge/platforms-Windows%20·%20Linux%20·%20macOS-0078d4?style=flat-square)](#install)

**English** · [Русский](README.ru.md)

</div>

## Why

Official desktop clients keep a lot in memory: a big media cache, a whole UI
toolkit, sometimes a bundled browser. Oxigram keeps only what is on screen.
The network, the database and the window are one small Rust program: the
interface is drawn by [Slint](https://slint.dev)'s software renderer, so there
is no browser engine and no GPU driver in memory.

- **About 10–30 MB of RAM** with the window open (more while a photo or a video
  is open full-window, back down when it closes). The live figure is always in
  the window title.
- **A ~15 MB executable**, installers of about 7 MB.
- Pictures are decoded at the size they are drawn and dropped when you leave
  the chat; long chats keep only the rows on screen.

## Features

<table>
<tr>
<td width="50%" valign="top">

**💬 Chats**
- Chat folders as tabs, Archive, pinned chats
- Search across chats and inside one (Ctrl+F)
- Mute, mark read/unread, archive, clear, leave
- Drafts per chat, contacts, a profile column
- "Typing…" in the header and the chat list

</td>
<td width="50%" valign="top">

**✉️ Messages**
- Replies, editing, forwarding, deleting, pinning
- Select several messages to forward or delete
- Reactions (custom emoji too), read ticks
- Colour emoji without a GPU
- `**bold**`, `__italic__`, `||spoiler||` shortcuts

</td>
</tr>
<tr>
<td valign="top">

**🖼️ Media**
- Photos and albums, a full-window viewer
- Video plays inside the app (Windows 8+, macOS, Linux with GStreamer)
- Voice messages with a waveform, music
- Send files by button, Ctrl+V or drag and drop; albums, progress, cancel
- "Memory saver" switches for avatars, pictures, GIFs, custom emoji

</td>
<td valign="top">

**🖥️ Desktop**
- Notifications that respect muted chats; "mute all" in the tray
- Tray mode, start with the system, one running copy
- SOCKS5 proxy, set up before the API keys
- Light and dark themes, text size
- Keyboard first: Ctrl+Tab, ↑ to edit, Ctrl+F, Esc

</td>
</tr>
</table>

**Not yet:** HTTP and MTProto proxies, animated (Lottie) stickers, calls,
stories, several accounts, recording voice messages, secret chats.

## Install

Installers are attached to the [releases](https://github.com/Timon2306/oxigram/releases):

| File | System |
|---|---|
| `oxigram-windows-x64-…-setup.exe` | Windows 10/11, 64-bit |
| `oxigram-windows-x86-…-setup.exe` | Windows 10/11, 32-bit |
| `oxigram-windows-arm64-…-setup.exe` | Windows 11 on ARM |
| `oxigram-linux-x64-….deb` | Ubuntu 22.04+, Debian 12+, Mint |
| `oxigram-linux-x64-….AppImage` | other distributions (glibc 2.35+) |
| `oxigram-macos-apple-silicon-….dmg` | Mac with M1/M2/M3/M4 |
| `oxigram-macos-intel-….dmg` | Mac with Intel |

The builds are not code-signed. On Windows, if SmartScreen warns, choose
**More info → Run anyway**. On macOS, right-click Oxigram → **Open** the
first time.

## Getting started

Oxigram ships **without** an `api_id` / `api_hash`; you bring your own.

1. Sign in at <https://my.telegram.org> → **API development tools**.
2. Create an application (any name) and copy `api_id` and `api_hash`.
3. Start Oxigram, paste them, sign in with your phone number.

Where Telegram is blocked, set up a SOCKS5 proxy first: **Set up proxy** on the
first screen (the keys can only be checked by connecting).

The keys and the session are stored only on your computer and are sent nowhere
except Telegram's servers.

## Build from source

You need [Rust](https://rustup.rs) (stable).

```sh
git clone https://github.com/Timon2306/oxigram
cd oxigram
cargo run --release -p oxigram-native
```

On Linux install the development packages first (Debian/Ubuntu names):

```sh
sudo apt install build-essential pkg-config libxkbcommon-dev libwayland-dev \
  libfontconfig1-dev libgtk-3-dev libayatana-appindicator3-dev libxdo-dev \
  libdbus-1-dev libasound2-dev libssl-dev
# video inside the app: also libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev,
# then build with --features gstreamer
```

Installers are made with [cargo-packager](https://github.com/crabnebula-dev/cargo-packager)
(`cargo install cargo-packager --locked`, then `cargo packager --release` in
`native/`). The GitHub workflow `.github/workflows/release.yml` builds all of
them; it only runs when started by hand.

<details>
<summary><b>Tests and project layout</b></summary>

<br>

```sh
cargo test -p oxigram-core -p oxigram-native   # unit tests
cargo test -- --ignored                        # also talks to Telegram's servers
```

```
core/src/            the backend (no UI inside)
  tg.rs              connection lifecycle, login, proxy
  chats.rs           dialogs, history, profiles, folders, media downloads
  actions.rs         pin, archive, react, edit, forward, delete, send files,
                     search, contacts
  updates.rs         live updates, notifications, the event stream
  proxy.rs           SOCKS5 settings and the connection check
  markup.rs          **bold** / __italic__ shortcuts to message entities
  media.rs           what an attachment is and which thumbnail to fetch
  store/             local SQLite: the session and the chat cache
  mem.rs             process memory measurement (window title)
native/              the app: Slint window and its Rust side
  ui/app.slint       the whole interface
  src/main.rs        start-up, chat list, history
  src/pictures.rs    pictures, albums, the viewer
  src/video/         video playback per system (Media Foundation,
                     AVFoundation, GStreamer)
  src/audio.rs       voice messages and music
  src/sending.rs     sending files
  src/…              reactions, search, folders, tray, notifications, proxy…
  vendor/            Slint's software renderer with colour emoji
```

Up to version 0.1 Oxigram drew its window with a WebView (Tauri + Svelte); since 0.2 it is fully native.

</details>

## Playing fair

Oxigram is an unofficial client built on the Telegram API and is not
affiliated with Telegram. It follows the
[API Terms of Service](https://core.telegram.org/api/terms): no "ghost mode",
no hidden read receipts or typing status, no keeping of deleted or
self-destructing messages.

## License

[MIT](LICENSE) © Timon2306

Use, change and share Oxigram freely, including in your own projects. The one
condition: keep the copyright notice from [LICENSE](LICENSE), crediting
Timon2306, in every copy and every project based on this code.

The interface is built with Slint under its Royalty-free license (Slint's
software renderer in `native/vendor/` carries the same licenses; see the files
there).

<a href="https://slint.dev"><img src="https://raw.githubusercontent.com/slint-ui/slint/master/logo/MadeWithSlint-logo-whitebg.png" height="48" alt="Made with Slint"></a>
