//! Text for service messages ("X joined the group", calls, pins…), which
//! Telegram sends as structured actions rather than as text.

use grammers_client::message::Message;
use grammers_client::peer::Peer;
use grammers_client::tl::enums::MessageAction as Action;

use crate::tr;

/// The line shown in the chat for a service message, or `None` for an
/// ordinary message.
pub fn text(message: &Message) -> Option<String> {
    let action = message.action()?;
    let you = message.outgoing();
    let sender_id = message.sender_id().and_then(|id| id.bare_id());
    let name = message.sender().and_then(Peer::name).unwrap_or_default();
    let n = if name.is_empty() { tr!("Someone") } else { name.to_owned() };

    Some(match action {
        Action::ChatCreate(a) => if you { tr!("You created the group «{}»", a.title) } else { tr!("{} created the group «{}»", n, a.title) },
        Action::ChannelCreate(a) => tr!("Channel «{}» created", a.title),
        Action::ChatEditTitle(a) => if you { tr!("You changed the group name to «{}»", a.title) } else { tr!("{} changed the group name to «{}»", n, a.title) },
        Action::ChatEditPhoto(_) => if you { tr!("You updated the group photo") } else { tr!("{} updated the group photo", n) },
        Action::ChatDeletePhoto => if you { tr!("You removed the group photo") } else { tr!("{} removed the group photo", n) },
        Action::ChatAddUser(a) => {
            if a.users.len() == 1 && Some(a.users[0]) == sender_id {
                if you { tr!("You joined the group") } else { tr!("{} joined the group", n) }
            } else {
                if you { tr!("You added members") } else { tr!("{} added members", n) }
            }
        }
        Action::ChatDeleteUser(a) => {
            if Some(a.user_id) == sender_id {
                if you { tr!("You left the group") } else { tr!("{} left the group", n) }
            } else {
                if you { tr!("You removed a member") } else { tr!("{} removed a member", n) }
            }
        }
        Action::ChatJoinedByLink(_) => if you { tr!("You joined via an invite link") } else { tr!("{} joined via an invite link", n) },
        Action::ChatJoinedByRequest => if you { tr!("You were accepted into the group") } else { tr!("{} was accepted into the group", n) },
        Action::ChatMigrateTo(_) | Action::ChannelMigrateFrom(_) => {
            tr!("The group was upgraded to a supergroup")
        }
        Action::PinMessage => if you { tr!("You pinned a message") } else { tr!("{} pinned a message", n) },
        Action::HistoryClear => tr!("History cleared"),
        Action::PhoneCall(a) => {
            let kind = match (a.video, you) {
                (true, true) => tr!("Outgoing video call"),
                (true, false) => tr!("Incoming video call"),
                (false, true) => tr!("Outgoing call"),
                (false, false) => tr!("Incoming call"),
            };
            match a.duration {
                Some(seconds) if seconds > 0 => {
                    format!("{kind} · {}:{:02}", seconds / 60, seconds % 60)
                }
                _ => kind,
            }
        }
        Action::ContactSignUp => if you { tr!("You are now on Telegram") } else { tr!("{} is now on Telegram", n) },
        Action::ScreenshotTaken => if you { tr!("You took a screenshot") } else { tr!("{} took a screenshot", n) },
        Action::GroupCall(_) | Action::GroupCallScheduled(_) | Action::InviteToGroupCall(_) => {
            tr!("Video chat")
        }
        Action::TopicCreate(a) => tr!("Topic «{}» created", a.title),
        Action::CustomAction(a) => a.message.clone(),
        _ => tr!("Service message"),
    })
}
