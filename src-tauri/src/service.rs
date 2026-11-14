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
            format!("Вы {you_form}")
        } else if name.is_empty() {
            capitalize(other_form)
        } else {
            format!("{name} {other_form}")
        }
    };

    Some(match action {
        Action::ChatCreate(a) => format!(
            "{} группу «{}»",
            who("создали", "создал(а)"),
            a.title
        ),
        Action::ChannelCreate(a) => format!("Создан канал «{}»", a.title),
        Action::ChatEditTitle(a) => format!(
            "{} название на «{}»",
            who("изменили", "изменил(а)"),
            a.title
        ),
        Action::ChatEditPhoto(_) => who("обновили фото", "обновил(а) фото"),
        Action::ChatDeletePhoto => who("удалили фото", "удалил(а) фото"),
        Action::ChatAddUser(a) => {
            if a.users.len() == 1 && Some(a.users[0]) == sender_id {
                who("вступили в группу", "вступил(а) в группу")
            } else {
                who("добавили участников", "добавил(а) участников")
            }
        }
        Action::ChatDeleteUser(a) => {
            if Some(a.user_id) == sender_id {
                who("покинули группу", "покинул(а) группу")
            } else {
                who("исключили участника", "исключил(а) участника")
            }
        }
        Action::ChatJoinedByLink(_) => who(
            "вступили по ссылке-приглашению",
            "вступил(а) по ссылке-приглашению",
        ),
        Action::ChatJoinedByRequest => who("приняты в группу", "принят(а) в группу"),
        Action::ChatMigrateTo(_) | Action::ChannelMigrateFrom(_) => {
            "Группа преобразована в супергруппу".to_owned()
        }
        Action::PinMessage => who("закрепили сообщение", "закрепил(а) сообщение"),
        Action::HistoryClear => "История очищена".to_owned(),
        Action::PhoneCall(a) => {
            let kind = match (a.video, you) {
                (true, true) => "Исходящий видеозвонок",
                (true, false) => "Входящий видеозвонок",
                (false, true) => "Исходящий звонок",
                (false, false) => "Входящий звонок",
            };
            match a.duration {
                Some(seconds) if seconds > 0 => {
                    format!("{kind} · {}:{:02}", seconds / 60, seconds % 60)
                }
                _ => kind.to_owned(),
            }
        }
        Action::ContactSignUp => who("теперь в Telegram", "теперь в Telegram"),
        Action::ScreenshotTaken => who("сделали снимок экрана", "сделал(а) снимок экрана"),
        Action::GroupCall(_) | Action::GroupCallScheduled(_) | Action::InviteToGroupCall(_) => {
            "Видеочат".to_owned()
        }
        Action::TopicCreate(a) => format!("Создана тема «{}»", a.title),
        Action::CustomAction(a) => a.message.clone(),
        _ => "Служебное сообщение".to_owned(),
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
        assert_eq!(capitalize("создал(а) группу"), "Создал(а) группу");
        assert_eq!(capitalize(""), "");
    }
}
