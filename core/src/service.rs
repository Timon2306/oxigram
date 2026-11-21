//! Text for service messages ("X joined the group", calls, pins…), which
//! Telegram sends as structured actions rather than as text.

use grammers_client::message::Message;
use grammers_client::peer::Peer;
use grammers_client::tl::enums::MessageAction as Action;

/// The line shown in the chat for a service message, or `None` for an
/// ordinary message.
pub fn text(message: &Message) -> Option<String> {
    let action = message.action()?;
    let you = message.outgoing();
    let sender_id = message.sender_id().and_then(|id| id.bare_id());
    let name = message.sender().and_then(Peer::name).unwrap_or_default();
    // "Вы создали" vs "Анна создал(а)": Russian verbs agree with the subject.
    let who = |you_form: &str, other_form: &str| -> String {
        if you {
            format!("You {you_form}")
        } else if name.is_empty() {
            capitalize(other_form)
        } else {
            format!("{name} {other_form}")
        }
    };

    Some(match action {
        Action::ChatCreate(a) => format!(
            "{} “{}”",
            who("created the group", "created the group"),
            a.title
        ),
        Action::ChannelCreate(a) => format!("Channel “{}” created", a.title),
        Action::ChatEditTitle(a) => format!(
            "{} the group name to “{}”",
            who("changed", "changed"),
            a.title
        ),
        Action::ChatEditPhoto(_) => who("updated the group photo", "updated the group photo"),
        Action::ChatDeletePhoto => who("removed the group photo", "removed the group photo"),
        Action::ChatAddUser(a) => {
            if a.users.len() == 1 && Some(a.users[0]) == sender_id {
                who("joined the group", "joined the group")
            } else {
                who("added members", "added members")
            }
        }
        Action::ChatDeleteUser(a) => {
            if Some(a.user_id) == sender_id {
                who("left the group", "left the group")
            } else {
                who("removed a member", "removed a member")
            }
        }
        Action::ChatJoinedByLink(_) => who(
            "joined the group via invite link",
            "joined the group via invite link",
        ),
        Action::ChatJoinedByRequest => who("were accepted to the group", "was accepted to the group"),
        Action::ChatMigrateTo(_) | Action::ChannelMigrateFrom(_) => {
            "The group was upgraded to a supergroup".to_owned()
        }
        Action::PinMessage => who("pinned a message", "pinned a message"),
        Action::HistoryClear => "History cleared".to_owned(),
        Action::PhoneCall(a) => {
            let kind = match (a.video, you) {
                (true, true) => "Outgoing video call",
                (true, false) => "Incoming video call",
                (false, true) => "Outgoing call",
                (false, false) => "Incoming call",
            };
            match a.duration {
                Some(seconds) if seconds > 0 => {
                    format!("{kind} · {}:{:02}", seconds / 60, seconds % 60)
                }
                _ => kind.to_owned(),
            }
        }
        Action::ContactSignUp => who("joined Telegram", "joined Telegram"),
        Action::ScreenshotTaken => who("took a screenshot", "took a screenshot"),
        Action::GroupCall(_) | Action::GroupCallScheduled(_) | Action::InviteToGroupCall(_) => {
            "Video chat".to_owned()
        }
        Action::TopicCreate(a) => format!("Topic “{}” created", a.title),
        Action::CustomAction(a) => a.message.clone(),
        _ => "Service message".to_owned(),
    })
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::capitalize;

    #[test]
    fn capitalizes_cyrillic() {
        assert_eq!(capitalize("created the group"), "Created the group");
        assert_eq!(capitalize(""), "");
    }
}
