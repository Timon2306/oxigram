//! `grammers_session::Session` on top of rusqlite.
//!
//! A port of grammers' built-in `SqliteSession` (same tables, same semantics),
//! which cannot be used directly because it links libsql. Every operation is a
//! short synchronous query, so the returned futures are already resolved.

use std::collections::HashMap;
use std::fmt;
use std::future::ready;
use std::net::{AddrParseError, Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};
use std::sync::{Mutex, MutexGuard};

use grammers_session::types::{
    ChannelKind, ChannelState, DcOption, PeerAuth, PeerId, PeerInfo, PeerKind, UpdateState,
    UpdatesState,
};
use grammers_session::{BoxFuture, Session};
use rusqlite::{Connection, OptionalExtension, params};

use super::Db;

/// The datacenter a fresh session talks to before Telegram redirects it.
const DEFAULT_DC: i32 = 2;

/// Statically known production datacenters (from `help.getConfig`), used until
/// the session learns the current addresses.
fn known_dc_options() -> [DcOption; 5] {
    let dc = |id, v4: [u8; 4], v6: [u16; 8]| DcOption {
        id,
        ipv4: SocketAddrV4::new(Ipv4Addr::from(v4), 443),
        ipv6: SocketAddrV6::new(Ipv6Addr::from(v6), 443, 0, 0),
        auth_key: None,
    };
    [
        dc(1, [149, 154, 175, 53], [0x2001, 0xb28, 0xf23d, 0xf001, 0, 0, 0, 0xa]),
        dc(2, [149, 154, 167, 41], [0x2001, 0x67c, 0x4e8, 0xf002, 0, 0, 0, 0xa]),
        dc(3, [149, 154, 175, 100], [0x2001, 0xb28, 0xf23d, 0xf003, 0, 0, 0, 0xa]),
        dc(4, [149, 154, 167, 92], [0x2001, 0x67c, 0x4e8, 0xf004, 0, 0, 0, 0xa]),
        dc(5, [91, 108, 56, 104], [0x2001, 0xb28, 0xf23f, 0xf005, 0, 0, 0, 0xa]),
    ]
}

// Bit layout of `peer_info.subtype`, shared with grammers' own storage.
const USER_SELF: u8 = 1;
const USER_BOT: u8 = 2;
const MEGAGROUP: u8 = 4;
const BROADCAST: u8 = 8;
const GIGAGROUP: u8 = 12;
const COMMUNITY: u8 = 16;

#[derive(Debug)]
pub enum SessionError {
    Poisoned,
    AddrParse(AddrParseError),
    Sql(rusqlite::Error),
    InvalidAuthKeyLength(usize),
}

impl std::error::Error for SessionError {}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Poisoned => write!(f, "session lock is poisoned"),
            Self::AddrParse(err) => write!(f, "invalid socket address in session: {err}"),
            Self::Sql(err) => write!(f, "session storage: {err}"),
            Self::InvalidAuthKeyLength(len) => {
                write!(f, "invalid auth_key length: expected 256, got {len}")
            }
        }
    }
}

impl From<AddrParseError> for SessionError {
    fn from(err: AddrParseError) -> Self {
        Self::AddrParse(err)
    }
}

impl From<rusqlite::Error> for SessionError {
    fn from(err: rusqlite::Error) -> Self {
        Self::Sql(err)
    }
}

type Result<T> = std::result::Result<T, SessionError>;

/// `home_dc_id` and `dc_option` are called on every request, so they are
/// served from memory.
struct Cache {
    home_dc: i32,
    dc_options: HashMap<i32, DcOption>,
}

pub struct SqliteSession {
    db: Db,
    cache: Mutex<Cache>,
}

impl SqliteSession {
    pub fn load(db: Db) -> Result<Self> {
        let cache = {
            let conn = db.lock().map_err(|_| SessionError::Poisoned)?;
            read_cache(&conn)?
        };
        Ok(Self {
            db,
            cache: Mutex::new(cache),
        })
    }

    /// Forgets the account: authorization keys, cached peers, update state
    /// and every cached dialog and message.
    /// The caller must have torn down every connection using this session.
    pub fn wipe(&self) -> Result<()> {
        self.conn()?.execute_batch(
            "BEGIN;
             DELETE FROM dc_home;
             DELETE FROM dc_option;
             DELETE FROM peer_info;
             DELETE FROM update_state;
             DELETE FROM channel_state;
             DELETE FROM dialogs;
             DELETE FROM messages;
             COMMIT;",
        )?;
        // Rewrites the file and empties the WAL: deleted chats and keys must
        // not stay readable in free pages.
        {
            let conn = self.conn()?;
            conn.execute_batch("VACUUM")?;
            let _ = conn.pragma_update(None, "wal_checkpoint", "TRUNCATE");
        }
        let mut cache = self.cache()?;
        cache.home_dc = DEFAULT_DC;
        cache.dc_options.clear();
        Ok(())
    }

    fn conn(&self) -> Result<MutexGuard<'_, Connection>> {
        self.db.lock().map_err(|_| SessionError::Poisoned)
    }

    fn cache(&self) -> Result<MutexGuard<'_, Cache>> {
        self.cache.lock().map_err(|_| SessionError::Poisoned)
    }

    fn store_home_dc(&self, dc_id: i32) -> Result<()> {
        self.cache()?.home_dc = dc_id;
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM dc_home", [])?;
        tx.execute("INSERT INTO dc_home VALUES (?1)", [dc_id])?;
        Ok(tx.commit()?)
    }

    fn store_dc_option(&self, dc_option: &DcOption) -> Result<()> {
        self.cache()?
            .dc_options
            .insert(dc_option.id, dc_option.clone());
        self.conn()?.execute(
            "INSERT OR REPLACE INTO dc_option VALUES (?1, ?2, ?3, enc(?4))",
            params![
                dc_option.id,
                dc_option.ipv4.to_string(),
                dc_option.ipv6.to_string(),
                dc_option.auth_key.as_ref().map(|key| key.as_slice()),
            ],
        )?;
        Ok(())
    }

    fn store_peers(&self, peers: &[PeerInfo]) -> Result<()> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        for peer in peers {
            // Fields already known must survive an update that lacks them.
            let merged = match read_peer(&tx, peer.id())? {
                Some(mut existing) => {
                    existing.extend_info(peer);
                    existing
                }
                None => peer.clone(),
            };
            write_peer(&tx, &merged)?;
        }
        Ok(tx.commit()?)
    }

    fn load_updates_state(&self) -> Result<UpdatesState> {
        let conn = self.conn()?;
        let mut state = conn
            .query_row(
                "SELECT pts, qts, date, seq FROM update_state LIMIT 1",
                [],
                |row| {
                    Ok(UpdatesState {
                        pts: row.get(0)?,
                        qts: row.get(1)?,
                        date: row.get(2)?,
                        seq: row.get(3)?,
                        channels: Vec::new(),
                    })
                },
            )
            .optional()?
            .unwrap_or_default();
        let mut stmt = conn.prepare("SELECT peer_id, pts FROM channel_state ORDER BY peer_id")?;
        state.channels = stmt
            .query_map([], |row| {
                Ok(ChannelState {
                    id: row.get(0)?,
                    pts: row.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(state)
    }

    fn store_update_state(&self, update: UpdateState) -> Result<()> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        match update {
            UpdateState::All(state) => {
                tx.execute("DELETE FROM update_state", [])?;
                tx.execute(
                    "INSERT INTO update_state VALUES (?1, ?2, ?3, ?4)",
                    params![state.pts, state.qts, state.date, state.seq],
                )?;
                tx.execute("DELETE FROM channel_state", [])?;
                for channel in state.channels {
                    tx.execute(
                        "INSERT INTO channel_state VALUES (?1, ?2)",
                        params![channel.id, channel.pts],
                    )?;
                }
            }
            UpdateState::Primary { pts, date, seq } => {
                let updated = tx.execute(
                    "UPDATE update_state SET pts = ?1, date = ?2, seq = ?3",
                    params![pts, date, seq],
                )?;
                if updated == 0 {
                    tx.execute(
                        "INSERT INTO update_state VALUES (?1, 0, ?2, ?3)",
                        params![pts, date, seq],
                    )?;
                }
            }
            UpdateState::Secondary { qts } => {
                let updated = tx.execute("UPDATE update_state SET qts = ?1", [qts])?;
                if updated == 0 {
                    tx.execute("INSERT INTO update_state VALUES (0, ?1, 0, 0)", [qts])?;
                }
            }
            UpdateState::Channel { id, pts } => {
                tx.execute(
                    "INSERT OR REPLACE INTO channel_state VALUES (?1, ?2)",
                    params![id, pts],
                )?;
            }
        }
        Ok(tx.commit()?)
    }
}

fn read_cache(conn: &Connection) -> Result<Cache> {
    let home_dc = conn
        .query_row("SELECT dc_id FROM dc_home LIMIT 1", [], |row| row.get(0))
        .optional()?
        .unwrap_or(DEFAULT_DC);

    let mut stmt = conn.prepare("SELECT dc_id, ipv4, ipv6, dec(auth_key) FROM dc_option")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i32>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<Vec<u8>>>(3)?,
        ))
    })?;

    let mut dc_options = HashMap::new();
    for row in rows {
        let (id, ipv4, ipv6, auth_key) = row?;
        let auth_key = match auth_key {
            None => None,
            Some(key) => Some(
                <[u8; 256]>::try_from(key)
                    .map_err(|key| SessionError::InvalidAuthKeyLength(key.len()))?,
            ),
        };
        dc_options.insert(
            id,
            DcOption {
                id,
                ipv4: ipv4.parse()?,
                ipv6: ipv6.parse()?,
                auth_key,
            },
        );
    }
    Ok(Cache {
        home_dc,
        dc_options,
    })
}

fn read_peer(conn: &Connection, peer: PeerId) -> rusqlite::Result<Option<PeerInfo>> {
    let map_row = |row: &rusqlite::Row<'_>| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<i64>>(1)?,
            row.get::<_, Option<i64>>(2)?,
        ))
    };
    let row = match peer.bot_api_dialog_id() {
        Some(peer_id) => conn
            .query_row(
                "SELECT peer_id, hash, subtype FROM peer_info WHERE peer_id = ?1",
                [peer_id],
                map_row,
            )
            .optional()?,
        // `PeerId::self_user()` has no numeric ID: find the row flagged as self.
        None => conn
            .query_row(
                "SELECT peer_id, hash, subtype FROM peer_info WHERE subtype & ?1 LIMIT 1",
                [USER_SELF],
                map_row,
            )
            .optional()?,
    };

    Ok(row.map(|(peer_id, hash, subtype)| {
        let subtype = subtype.map(|s| s as u8);
        let auth = hash.map(PeerAuth::from_hash);
        match peer.kind() {
            // For users the stored dialog ID is the bare ID.
            PeerKind::User => PeerInfo::User {
                id: peer_id,
                auth,
                bot: subtype.map(|s| s & USER_BOT != 0),
                is_self: subtype.map(|s| s & USER_SELF != 0),
            },
            PeerKind::Chat => PeerInfo::Chat {
                id: peer.bare_id().unwrap_or_default(),
            },
            PeerKind::Channel => PeerInfo::Channel {
                id: peer.bare_id().unwrap_or_default(),
                auth,
                kind: subtype.and_then(channel_kind),
            },
        }
    }))
}

fn write_peer(conn: &Connection, peer: &PeerInfo) -> rusqlite::Result<()> {
    let Some(peer_id) = peer.id().bot_api_dialog_id() else {
        return Ok(());
    };
    conn.execute(
        "INSERT OR REPLACE INTO peer_info VALUES (?1, ?2, ?3)",
        params![peer_id, peer.auth().map(PeerAuth::hash), subtype_of(peer)],
    )?;
    Ok(())
}

fn subtype_of(peer: &PeerInfo) -> Option<u8> {
    match peer {
        PeerInfo::User { bot, is_self, .. } => {
            let bits = if bot.unwrap_or_default() { USER_BOT } else { 0 }
                | if is_self.unwrap_or_default() { USER_SELF } else { 0 };
            (bits != 0).then_some(bits)
        }
        PeerInfo::Chat { .. } => None,
        PeerInfo::Channel { kind, .. } => kind.map(|kind| match kind {
            ChannelKind::Megagroup => MEGAGROUP,
            ChannelKind::Broadcast => BROADCAST,
            ChannelKind::Gigagroup => GIGAGROUP,
            ChannelKind::Community => COMMUNITY,
        }),
    }
}

fn channel_kind(subtype: u8) -> Option<ChannelKind> {
    if subtype & COMMUNITY != 0 {
        Some(ChannelKind::Community)
    } else if subtype & GIGAGROUP == GIGAGROUP {
        Some(ChannelKind::Gigagroup)
    } else if subtype & BROADCAST != 0 {
        Some(ChannelKind::Broadcast)
    } else if subtype & MEGAGROUP != 0 {
        Some(ChannelKind::Megagroup)
    } else {
        None
    }
}

fn resolved<'a, T: Send + 'a>(value: T) -> BoxFuture<'a, T> {
    Box::pin(ready(value))
}

impl Session for SqliteSession {
    type Error = SessionError;

    fn home_dc_id(&self) -> Result<i32> {
        Ok(self.cache()?.home_dc)
    }

    fn set_home_dc_id(&self, dc_id: i32) -> BoxFuture<'_, Result<()>> {
        resolved(self.store_home_dc(dc_id))
    }

    fn dc_option(&self, dc_id: i32) -> Result<Option<DcOption>> {
        Ok(self.cache()?.dc_options.get(&dc_id).cloned().or_else(|| {
            known_dc_options()
                .into_iter()
                .find(|dc_option| dc_option.id == dc_id)
        }))
    }

    fn set_dc_option(&self, dc_option: &DcOption) -> BoxFuture<'_, Result<()>> {
        resolved(self.store_dc_option(dc_option))
    }

    fn peer(&self, peer: PeerId) -> BoxFuture<'_, Result<Option<PeerInfo>>> {
        resolved(
            self.conn()
                .and_then(|conn| Ok(read_peer(&conn, peer)?)),
        )
    }

    fn cache_peer(&self, peer: PeerInfo) -> BoxFuture<'_, Result<()>> {
        resolved(self.store_peers(std::slice::from_ref(&peer)))
    }

    fn cache_peers(&self, peers: Vec<PeerInfo>) -> BoxFuture<'_, Result<()>> {
        resolved(self.store_peers(&peers))
    }

    fn updates_state(&self) -> BoxFuture<'_, Result<UpdatesState>> {
        resolved(self.load_updates_state())
    }

    fn set_update_state(&self, update: UpdateState) -> BoxFuture<'_, Result<()>> {
        resolved(self.store_update_state(update))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store;

    fn block_on<F: Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(future)
    }

    fn user(id: i64) -> PeerId {
        PeerId::user(id).unwrap()
    }

    fn channel(id: i64) -> PeerId {
        PeerId::channel(id).unwrap()
    }

    // Mirrors grammers' own `exercise_sqlite_session` so both storages are
    // held to the same contract.
    #[test]
    fn behaves_like_the_upstream_storage() {
        block_on(async {
            let session = SqliteSession::load(store::open(":memory:").unwrap()).unwrap();

            assert_eq!(session.home_dc_id().unwrap(), DEFAULT_DC);
            session.set_home_dc_id(DEFAULT_DC + 1).await.unwrap();
            assert_eq!(session.home_dc_id().unwrap(), DEFAULT_DC + 1);

            let known = known_dc_options();
            assert_eq!(
                session.dc_option(known[0].id).unwrap(),
                Some(known[0].clone())
            );
            let new_dc_option = DcOption {
                id: 6,
                ipv4: SocketAddrV4::new(Ipv4Addr::from_bits(0), 1),
                ipv6: SocketAddrV6::new(Ipv6Addr::from_bits(0), 1, 0, 0),
                auth_key: Some([1; 256]),
            };
            assert_eq!(session.dc_option(new_dc_option.id).unwrap(), None);
            session.set_dc_option(&new_dc_option).await.unwrap();
            assert_eq!(
                session.dc_option(new_dc_option.id).unwrap(),
                Some(new_dc_option)
            );

            assert_eq!(session.peer(PeerId::self_user()).await.unwrap(), None);
            assert_eq!(session.peer(user(1)).await.unwrap(), None);
            let me = PeerInfo::User {
                id: 1,
                auth: None,
                bot: Some(true),
                is_self: Some(true),
            };
            session.cache_peer(me.clone()).await.unwrap();
            assert_eq!(
                session.peer(PeerId::self_user()).await.unwrap(),
                Some(me.clone())
            );
            assert_eq!(session.peer(user(1)).await.unwrap(), Some(me));

            assert_eq!(session.peer(channel(1)).await.unwrap(), None);
            let broadcast = PeerInfo::Channel {
                id: 1,
                auth: Some(PeerAuth::from_hash(-1)),
                kind: Some(ChannelKind::Broadcast),
            };
            session.cache_peer(broadcast.clone()).await.unwrap();
            assert_eq!(session.peer(channel(1)).await.unwrap(), Some(broadcast));

            let peers = vec![
                PeerInfo::Channel {
                    id: 2,
                    auth: Some(PeerAuth::from_hash(-2)),
                    kind: Some(ChannelKind::Megagroup),
                },
                PeerInfo::Channel {
                    id: 3,
                    auth: Some(PeerAuth::from_hash(-3)),
                    kind: None,
                },
            ];
            session.cache_peers(peers.clone()).await.unwrap();
            for peer in &peers {
                assert_eq!(session.peer(peer.id()).await.unwrap().as_ref(), Some(peer));
            }

            // A field going from `Some` to `None` keeps the old `Some`.
            session
                .cache_peers(vec![
                    PeerInfo::Channel {
                        id: 2,
                        auth: None,
                        kind: Some(ChannelKind::Broadcast),
                    },
                    PeerInfo::Channel {
                        id: 3,
                        auth: Some(PeerAuth::from_hash(-30)),
                        kind: None,
                    },
                ])
                .await
                .unwrap();
            assert_eq!(
                session.peer(channel(2)).await.unwrap(),
                Some(PeerInfo::Channel {
                    id: 2,
                    auth: Some(PeerAuth::from_hash(-2)),
                    kind: Some(ChannelKind::Broadcast),
                })
            );
            assert_eq!(
                session.peer(channel(3)).await.unwrap(),
                Some(PeerInfo::Channel {
                    id: 3,
                    auth: Some(PeerAuth::from_hash(-30)),
                    kind: None,
                })
            );

            assert_eq!(
                session.updates_state().await.unwrap(),
                UpdatesState::default()
            );
            session
                .set_update_state(UpdateState::All(UpdatesState {
                    pts: 1,
                    qts: 2,
                    date: 3,
                    seq: 4,
                    channels: vec![
                        ChannelState { id: 5, pts: 6 },
                        ChannelState { id: 7, pts: 8 },
                    ],
                }))
                .await
                .unwrap();
            session
                .set_update_state(UpdateState::Primary {
                    pts: 2,
                    date: 4,
                    seq: 5,
                })
                .await
                .unwrap();
            session
                .set_update_state(UpdateState::Secondary { qts: 3 })
                .await
                .unwrap();
            session
                .set_update_state(UpdateState::Channel { id: 7, pts: 9 })
                .await
                .unwrap();
            assert_eq!(
                session.updates_state().await.unwrap(),
                UpdatesState {
                    pts: 2,
                    qts: 3,
                    date: 4,
                    seq: 5,
                    channels: vec![
                        ChannelState { id: 5, pts: 6 },
                        ChannelState { id: 7, pts: 9 },
                    ],
                }
            );
        });
    }

    #[test]
    fn survives_reopen_and_wipe() {
        let path = std::env::temp_dir().join(format!("oxigram-session-test-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let dc_option = DcOption {
            id: 4,
            auth_key: Some([7; 256]),
            ..known_dc_options()[3].clone()
        };

        block_on(async {
            let session = SqliteSession::load(store::open(&path).unwrap()).unwrap();
            session.set_home_dc_id(4).await.unwrap();
            session.set_dc_option(&dc_option).await.unwrap();
        });

        let session = SqliteSession::load(store::open(&path).unwrap()).unwrap();
        assert_eq!(session.home_dc_id().unwrap(), 4);
        assert_eq!(session.dc_option(4).unwrap(), Some(dc_option));

        session.wipe().unwrap();
        assert_eq!(session.home_dc_id().unwrap(), DEFAULT_DC);
        assert_eq!(session.dc_option(4).unwrap().unwrap().auth_key, None);
        drop(session);

        let session = SqliteSession::load(store::open(&path).unwrap()).unwrap();
        assert_eq!(session.home_dc_id().unwrap(), DEFAULT_DC);
        drop(session);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }
}
