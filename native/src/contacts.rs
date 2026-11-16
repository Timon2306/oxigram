//! The contacts window (burger menu): search, avatar, name and last seen;
//! a click opens the chat, even with someone it has no messages with yet.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use oxigram_core::actions;
use oxigram_core::chats;
use oxigram_core::store::chats::{DialogRow, Presence};
use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::{AppWindow, ContactItem, format, open_chat, run, tg, ui, with_app};

/// The window draws avatars at this size, in logical pixels.
const AVATAR_PX: f32 = 42.0;

struct Entry {
    id: i64,
    title: String,
    photo_id: Option<String>,
    presence: Option<Presence>,
}

#[derive(Default)]
struct State {
    all: Vec<Entry>,
    /// Indexes into `all` of the rows on screen (the search narrows them).
    shown: Vec<usize>,
    needle: String,
    model: Rc<VecModel<ContactItem>>,
    photos: HashMap<i64, slint::Image>,
    pending: HashSet<i64>,
    loaded: bool,
    error: String,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn item(entry: &Entry, photo: Option<&slint::Image>) -> ContactItem {
    let title = if entry.title.is_empty() { "Без имени".to_owned() } else { entry.title.clone() };
    ContactItem {
        initial: format::initial(&title).into(),
        title: title.into(),
        status: format::presence_text(entry.presence.as_ref()).into(),
        online: format::is_online(entry.presence.as_ref()),
        color: format::avatar_color(entry.id),
        photo: photo.cloned().unwrap_or_default(),
        has_photo: photo.is_some(),
    }
}

/// Fills the window's list from what is loaded and what the search says.
fn refresh() {
    let Some(ui) = ui() else { return };
    // The borrow ends before the model changes: new rows ask for their avatars.
    let (model, items, note) = STATE.with(|state| {
        let mut state = state.borrow_mut();
        let needle = state.needle.clone();
        let shown: Vec<usize> = state
            .all
            .iter()
            .enumerate()
            .filter(|(_, e)| needle.is_empty() || e.title.to_lowercase().contains(&needle))
            .map(|(index, _)| index)
            .collect();
        let items: Vec<ContactItem> =
            shown.iter().map(|&i| item(&state.all[i], state.photos.get(&state.all[i].id))).collect();
        state.shown = shown;
        let note = if !state.error.is_empty() {
            state.error.clone()
        } else if !state.loaded {
            "Загрузка…".to_owned()
        } else if !needle.is_empty() {
            "Ничего не найдено".to_owned()
        } else {
            "Контактов нет".to_owned()
        };
        (state.model.clone(), items, note)
    });
    ui.set_contacts_note(note.into());
    model.set_vec(items);
}

fn open() {
    let Some(ui) = ui() else { return };
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.needle.clear();
        state.loaded = false;
        state.error.clear();
        state.all.clear();
        state.shown.clear();
    });
    ui.set_contacts_open(true);
    refresh();
    run(async { actions::contacts(tg()).await }, |found| {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.loaded = true;
            match found {
                Ok(list) => {
                    state.all = list
                        .into_iter()
                        .map(|c| Entry { id: c.id, title: c.title, photo_id: c.photo_id, presence: c.presence })
                        .collect();
                }
                Err(err) => state.error = format::error(&err),
            }
        });
        refresh();
    });
}

/// Opens the private chat with a contact, even one with no messages yet: it
/// gets a placeholder row at the top of the list until Telegram knows it.
fn pick(index: usize) {
    let Some((id, title, photo_id, presence)) = STATE.with(|state| {
        let state = state.borrow();
        let entry = state.all.get(*state.shown.get(index)?)?;
        Some((entry.id, entry.title.clone(), entry.photo_id.clone(), entry.presence.clone()))
    }) else {
        return;
    };
    if let Some(ui) = ui() {
        ui.set_contacts_open(false);
    }
    with_app(|app| {
        app.show_archive = false;
        if app.dialog(id).is_none() {
            app.dialogs.insert(
                0,
                DialogRow {
                    id,
                    kind: "user".into(),
                    title,
                    last_text: String::new(),
                    last_date: 0,
                    last_outgoing: false,
                    last_id: 0,
                    unread: 0,
                    pinned: false,
                    archived: false,
                    photo_id,
                    read_outbox: 0,
                    unread_reactions: 0,
                    muted: false,
                    contact: true,
                    bot: false,
                    presence,
                    can_post: true,
                },
            );
        }
    });
    open_chat(id);
}

/// Loads the avatar of the row at `index` (when "Фото на аватарках" is on).
fn want_avatar(index: usize) {
    let Some((id, photo_id, px)) = STATE.with(|state| {
        let mut state = state.borrow_mut();
        let entry = state.all.get(*state.shown.get(index)?)?;
        let (id, photo_id) = (entry.id, entry.photo_id.clone()?);
        if state.photos.contains_key(&id) || state.pending.contains(&id) {
            return None;
        }
        if !with_app(|app| app.settings.avatars).unwrap_or(true) {
            return None;
        }
        state.pending.insert(id);
        let scale = ui().map_or(1.0, |ui| ui.window().scale_factor());
        Some((id, photo_id, (AVATAR_PX * scale).ceil() as u32))
    }) else {
        return;
    };
    run(
        async move {
            let path = chats::avatar(tg(), id, &photo_id).await.ok()?;
            tokio::task::spawn_blocking(move || {
                let picture = image::open(path).ok()?;
                let small = picture.resize_to_fill(px, px, image::imageops::FilterType::Triangle);
                Some(crate::circle(small.to_rgba8()))
            })
            .await
            .ok()
            .flatten()
        },
        move |pixels| {
            let Some(pixels) = pixels else { return };
            let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                pixels.as_raw(),
                pixels.width(),
                pixels.height(),
            );
            let photo = slint::Image::from_rgba8(buffer);
            STATE.with(|state| {
                let mut state = state.borrow_mut();
                state.pending.remove(&id);
                state.photos.insert(id, photo.clone());
                for (row, &i) in state.shown.iter().enumerate() {
                    if state.all[i].id == id
                        && let Some(mut data) = state.model.row_data(row)
                    {
                        data.photo = photo.clone();
                        data.has_photo = true;
                        state.model.set_row_data(row, data);
                    }
                }
            });
        },
    );
}

/// Forgets the account's contacts (sign-out).
pub fn clear() {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.all.clear();
        state.shown.clear();
        state.photos.clear();
        state.pending.clear();
        state.loaded = false;
    });
    refresh();
}

pub fn wire(window: &AppWindow) {
    STATE.with(|state| window.set_contacts_items(ModelRc::from(state.borrow().model.clone())));
    window.on_open_contacts(open);
    window.on_contacts_filter(|text| {
        STATE.with(|state| state.borrow_mut().needle = text.trim().to_lowercase());
        refresh();
    });
    window.on_contacts_pick(|index| pick(index.max(0) as usize));
    window.on_contacts_avatar(|index| want_avatar(index.max(0) as usize));
}
