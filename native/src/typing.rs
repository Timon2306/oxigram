//! "typing…": who is typing where. Display only; the user's own typing
//! status is sent (or not) elsewhere, exactly as before.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// An entry lives this long unless the same status arrives again.
pub const TTL: Duration = Duration::from_secs(6);

#[derive(Clone, Debug, PartialEq)]
struct Entry {
    user: i64,
    action: &'static str,
    at: Instant,
}

#[derive(Default)]
pub struct Typing {
    by_peer: HashMap<i64, Vec<Entry>>,
}

impl Typing {
    /// Records an update; "cancel" removes at once. True if the visible
    /// text of `peer` may have changed.
    pub fn update(&mut self, peer: i64, user: i64, action: &'static str, now: Instant) -> bool {
        if action == "cancel" {
            return self.remove(peer, user);
        }
        let list = self.by_peer.entry(peer).or_default();
        match list.iter_mut().find(|e| e.user == user) {
            Some(e) => {
                let changed = e.action != action;
                e.action = action;
                e.at = now;
                changed
            }
            None => {
                list.push(Entry { user, action, at: now });
                true
            }
        }
    }

    /// A message from `user` ends their typing.
    pub fn remove(&mut self, peer: i64, user: i64) -> bool {
        let Some(list) = self.by_peer.get_mut(&peer) else { return false };
        let before = list.len();
        list.retain(|e| e.user != user);
        let changed = list.len() != before;
        if list.is_empty() {
            self.by_peer.remove(&peer);
        }
        changed
    }

    /// Drops expired entries; returns the chats whose text changed.
    pub fn prune(&mut self, now: Instant) -> Vec<i64> {
        let mut changed = Vec::new();
        self.by_peer.retain(|peer, list| {
            let before = list.len();
            list.retain(|e| now.duration_since(e.at) < TTL);
            if list.len() != before {
                changed.push(*peer);
            }
            !list.is_empty()
        });
        changed
    }

    pub fn is_empty(&self) -> bool {
        self.by_peer.is_empty()
    }

    /// The line for a chat, if anyone types there. `private` chats never
    /// name the person; `name` resolves a user ID to a display name.
    pub fn line(&self, peer: i64, private: bool, name: impl Fn(i64) -> Option<String>) -> Option<String> {
        let list = self.by_peer.get(&peer).filter(|l| !l.is_empty())?;
        if private {
            return Some(action_text(list[0].action).to_owned());
        }
        if list.len() == 1 {
            let e = &list[0];
            return Some(match name(e.user) {
                Some(n) => format!("{} is {}", first_word(&n), action_text(e.action)),
                None => action_text(e.action).to_owned(),
            });
        }
        let names: Vec<String> =
            list.iter().filter_map(|e| name(e.user)).map(|n| first_word(&n).to_owned()).collect();
        if list.len() == 2 && names.len() == 2 {
            return Some(format!("{} and {} are typing…", names[0], names[1]));
        }
        let n = list.len() as i64;
        Some(format!("{n} people are typing…"))
    }
}

fn first_word(name: &str) -> &str {
    name.split_whitespace().next().unwrap_or(name)
}

/// "typing…", "recording a voice message…" and so on (as the Tauri build).
pub fn action_text(action: &str) -> &'static str {
    match action {
        "voice" => "recording a voice message…",
        "round" => "recording a video message…",
        "video" => "sending a video…",
        "photo" => "sending a photo…",
        "file" => "sending a file…",
        "sticker" => "choosing a sticker…",
        _ => "typing…",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(id: i64) -> Option<String> {
        match id {
            1 => Some("John Smith".into()),
            2 => Some("Maria".into()),
            _ => None,
        }
    }

    #[test]
    fn private_and_group_wording() {
        let (mut t, now) = (Typing::default(), Instant::now());
        t.update(10, 1, "typing", now);
        assert_eq!(t.line(10, true, names).unwrap(), "typing…");
        assert_eq!(t.line(10, false, names).unwrap(), "John is typing…");
        t.update(10, 3, "voice", now);
        assert_eq!(t.line(10, false, names).unwrap(), "2 people are typing…");
        t.update(10, 2, "typing", now);
        assert_eq!(t.line(10, false, names).unwrap(), "3 people are typing…");
        t.remove(10, 3);
        assert_eq!(t.line(10, false, names).unwrap(), "John and Maria are typing…");
        assert!(t.line(11, false, names).is_none());
    }

    #[test]
    fn actions_and_unknown_name() {
        let (mut t, now) = (Typing::default(), Instant::now());
        t.update(10, 9, "photo", now);
        assert_eq!(t.line(10, false, names).unwrap(), "sending a photo…");
        t.update(10, 9, "sticker", now);
        assert_eq!(t.line(10, false, names).unwrap(), "choosing a sticker…");
    }

    #[test]
    fn expiry_cancel_and_repeat() {
        let (mut t, now) = (Typing::default(), Instant::now());
        assert!(t.update(10, 1, "typing", now));
        assert!(!t.update(10, 1, "typing", now + Duration::from_secs(4)));
        assert!(t.prune(now + Duration::from_secs(9)).is_empty());
        assert_eq!(t.prune(now + Duration::from_secs(11)), vec![10]);
        assert!(t.is_empty());
        t.update(10, 1, "typing", now);
        assert!(t.update(10, 1, "cancel", now));
        assert!(t.is_empty());
        assert!(!t.remove(10, 1));
    }
}
