//! Everything Oxigram does that does not depend on a window toolkit: the
//! MTProto connection and login (grammers), the SQLite cache, chats,
//! messages, media and live updates. The native window (`native/`) is a thin
//! front end over this crate.

pub mod actions;
pub mod chats;
pub mod config;
pub mod error;
pub mod i18n;
pub mod markup;
pub mod media;
pub mod mem;
pub mod proxy;
mod proxy_http;
mod proxy_ws;
pub mod service;
pub mod store;
pub mod tg;
pub mod update;
pub mod updates;
pub mod vault;
