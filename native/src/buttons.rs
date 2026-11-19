//! Inline buttons under messages (bots, channels): links through the link
//! dialog, callbacks answered by the bot (a toast, or a dialog for alerts),
//! copy buttons, profile buttons. The rest works only in Telegram's apps.

use oxigram_core::actions;
use oxigram_core::store::chats::{InlineButton, MessageRow};
use slint::{ModelRc, VecModel};

use crate::{AppWindow, ButtonRow, KeyButton, format, run, tg, ui, with_app};

pub fn wire(window: &AppWindow) {
    window.on_press_button(press);
}

/// The rows of a message's buttons for its bubble.
pub fn rows(m: &MessageRow) -> ModelRc<ButtonRow> {
    let rows: Vec<ButtonRow> = m
        .buttons
        .iter()
        .enumerate()
        .map(|(r, row)| ButtonRow {
            items: ModelRc::new(VecModel::from(
                row.iter()
                    .enumerate()
                    .map(|(c, b)| KeyButton {
                        text: b.text.clone().into(),
                        link: b.kind == "url",
                        row: r as i32,
                        col: c as i32,
                    })
                    .collect::<Vec<_>>(),
            )),
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

fn button(id: i32, row: i32, col: i32) -> Option<(MessageRow, InlineButton)> {
    with_app(|app| {
        let m = app.messages.iter().find(|m| m.id == id)?.clone();
        let b = m.buttons.get(row.max(0) as usize)?.get(col.max(0) as usize)?.clone();
        Some((m, b))
    })
    .flatten()
}

fn press(id: i32, row: i32, col: i32) {
    let Some((m, b)) = button(id, row, col) else { return };
    match b.kind.as_str() {
        // A button's address is not visible beforehand: asked, as hidden links.
        "url" => crate::links::confirm_url(&b.value),
        "copy" => {
            if let Some(ui) = ui() {
                ui.invoke_copy_text(b.value.into());
            }
            crate::menus::toast(oxigram_core::tr!("Copied"));
        }
        "user" => {
            if let Ok(user) = b.value.parse::<i64>() {
                crate::open_chat(user);
            }
        }
        "callback" => {
            let (peer, data) = (m.peer_id, b.value.clone());
            run(async move { actions::press_button(tg(), peer, id, &data).await }, |result| match result {
                Ok(answer) => {
                    if let Some(text) = answer.message {
                        if answer.alert {
                            crate::menus::confirm(&text, "", "OK", "", |_| {});
                        } else {
                            crate::menus::toast(&text);
                        }
                    }
                    if let Some(url) = answer.url {
                        crate::links::confirm_url(&url);
                    }
                }
                Err(err) => crate::menus::toast(format::error(&err)),
            });
        }
        _ => crate::menus::toast(oxigram_core::tr!("This button works only in the Telegram app")),
    }
}
