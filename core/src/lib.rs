//! Everything Oxigram does that does not depend on a window toolkit: the
//! MTProto connection and login (grammers), the SQLite cache, chats,
//! messages, media and live updates. Both the Tauri and the native build
//! are thin front ends over this crate.

pub mod actions;
pub mod chats;
pub mod config;
pub mod error;
pub mod markup;
pub mod media;
pub mod mem;
pub mod proxy;
pub mod service;
pub mod store;
pub mod tg;
pub mod updates;
