//! Oxigram with a native Slint window instead of a WebView.
//!
//! Threads: the Slint event loop owns the window and all view state (`App`,
//! kept in a thread-local); a small tokio runtime runs `oxigram-core`. Work
//! goes to tokio with [`run`], and results come back through
//! `slint::invoke_from_event_loop`, so the view state is only ever touched on
//! the UI thread and needs no locks.

// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod context_menu;
mod format;
mod menus;
mod messages;
mod pictures;
mod settings;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::Duration;

use oxigram_core::chats;
use oxigram_core::error::UiError;
use oxigram_core::store::chats::{DialogRow, MessageRow};
use oxigram_core::tg::{Telegram, UiState};
use oxigram_core::updates::{EventSink, UiEvent};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

slint::include_modules!();

/// Same folders as the Tauri build (its bundle identifier), so both share the
/// keys, the session and the cache. The lock in `oxigram-core` keeps them
/// from running at the same time.
const APP_ID: &str = "app.oxigram.desktop";
const HISTORY_PAGE: usize = 50;
const TITLE_EVERY: Duration = Duration::from_secs(3);
/// Messages closer than this (seconds) from one sender form one run.
const RUN_GAP: i64 = 300;
/// Decoded avatars kept in memory; the files stay in the disk cache.
const AVATAR_CACHE: usize = 160;
/// The largest avatar drawn (the list's 48 px circle), in logical pixels.
const AVATAR_PX: f32 = 48.0;
/// Background paging stops here; scrolling further still loads more.
const MAX_DIALOGS: usize = 2000;

static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
static TELEGRAM: OnceLock<Telegram> = OnceLock::new();

fn tg() -> &'static Telegram {
    TELEGRAM.get().expect("set in main")
}

/// Runs `work` on tokio and hands its result to `done` on the UI thread.
fn run<T: Send + 'static>(
    work: impl Future<Output = T> + Send + 'static,
    done: impl FnOnce(T) + Send + 'static,
) {
    RUNTIME.get().expect("set in main").spawn(async move {
        let value = work.await;
        let _ = slint::invoke_from_event_loop(move || done(value));
    });
}

/// View state, owned by the UI thread.
struct App {
    ui: slint::Weak<AppWindow>,
    /// Every dialog, in list order (pinned first, then newest).
    dialogs: Vec<DialogRow>,
    /// Peer IDs of the rows on screen, which search and the archive narrow.
    shown: Vec<i64>,
    dialog_model: Rc<VecModel<DialogItem>>,
    filter: String,
    show_archive: bool,
    dialogs_done: bool,
    dialogs_loading: bool,
    /// Avatars shrunk to their circle, most recently used last.
    avatars: HashMap<i64, slint::Image>,
    avatar_order: VecDeque<i64>,
    avatar_pending: HashSet<i64>,
    chat: Option<i64>,
    /// The open chat's messages, oldest first, one per row of `message_model`.
    messages: Vec<MessageRow>,
    message_model: Rc<VecModel<MessageItem>>,
    history_done: bool,
    history_loading: bool,
    /// Bumped when another chat opens, so late replies for the old one are dropped.
    generation: u64,
    compose: messages::Compose,
    /// Peer IDs of the forward picker's rows, and the messages it forwards.
    forward_shown: Vec<i64>,
    forward_ids: Vec<i32>,
    /// Waits for the confirm dialog; gets the checkbox state when accepted.
    pending: Option<Box<dyn FnOnce(bool)>>,
    me_id: i64,
    me_photo_id: Option<String>,
    settings: settings::Settings,
    /// The top chats are prefetched once per start.
    prefetched: bool,
    /// What the open right-click menu acts on.
    menu_target: Option<context_menu::MenuTarget>,
    /// Decoded pictures of the open chat's messages.
    thumbs: pictures::Thumbs,
    /// The full-window picture viewer, when open.
    viewer: Option<pictures::Viewer>,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Runs `f` on the view state. Slint may call back into Rust while a model
/// is being changed; such nested calls are skipped rather than panicking.
fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|app| match app.try_borrow_mut() {
        Ok(mut app) => app.as_mut().map(f),
        Err(_) => None,
    })
}

/// Live events from the update task, delivered on the UI thread.
struct WindowSink;

impl EventSink for WindowSink {
    fn send(&self, events: Vec<UiEvent>) -> bool {
        slint::invoke_from_event_loop(move || {
            with_app(|app| app.apply_events(events));
        })
        .is_ok()
    }
}

/// Picks the renderer once, before the first window: OpenGL (femtovg) when the
/// "GPU rendering" setting is on, software otherwise. `SLINT_BACKEND` in the
/// environment overrides this (used for measurements).
fn select_renderer() {
    if std::env::var_os("SLINT_BACKEND").is_some() {
        return;
    }
    let select = |renderer: &str| {
        slint::BackendSelector::new()
            .backend_name("winit".into())
            .renderer_name(renderer.into())
            .select()
    };
    #[cfg(feature = "gpu")]
    if settings::Settings::load().gpu_render && select("femtovg").is_ok() {
        return;
    }
    let _ = select("software");
}

fn main() {
    // OXIGRAM_HOME puts everything in one folder: a clean profile for tests
    // and measurements that never touches the real account.
    let dirs = match std::env::var_os("OXIGRAM_HOME") {
        Some(home) => {
            let home = std::path::PathBuf::from(home);
            (Some(home.clone()), Some(home.clone()), Some(home))
        }
        None => (dirs::config_dir(), dirs::data_dir(), dirs::cache_dir()),
    };
    let (Some(config), Some(data), Some(cache)) = dirs else {
        eprintln!("no per-user folders on this system");
        return;
    };
    settings::set_dir(config.join(APP_ID));
    let _ = TELEGRAM.set(Telegram::new(
        config.join(APP_ID),
        data.join(APP_ID),
        cache.join(APP_ID).join("media"),
    ));
    // Two workers are plenty for one connection and a few downloads.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(512 * 1024)
        .enable_all()
        .build()
        .expect("tokio runtime");
    let _ = RUNTIME.set(runtime);

    select_renderer();
    let ui = AppWindow::new().expect("window");
    let dialog_model = Rc::new(VecModel::default());
    let message_model = Rc::new(VecModel::default());
    ui.set_dialogs(ModelRc::from(dialog_model.clone()));
    ui.set_messages(ModelRc::from(message_model.clone()));
    APP.with(|app| {
        *app.borrow_mut() = Some(App {
            ui: ui.as_weak(),
            dialogs: Vec::new(),
            shown: Vec::new(),
            dialog_model,
            filter: String::new(),
            show_archive: false,
            dialogs_done: false,
            dialogs_loading: false,
            avatars: HashMap::new(),
            avatar_order: VecDeque::new(),
            avatar_pending: HashSet::new(),
            chat: None,
            messages: Vec::new(),
            message_model,
            history_done: false,
            history_loading: false,
            generation: 0,
            compose: messages::Compose::None,
            forward_shown: Vec::new(),
            forward_ids: Vec::new(),
            pending: None,
            me_id: 0,
            me_photo_id: None,
            settings: settings::Settings::load(),
            prefetched: false,
            menu_target: None,
            thumbs: pictures::Thumbs::default(),
            viewer: None,
        })
    });
    let bench = std::env::var_os("OXIGRAM_BENCH").is_some();
    ui.set_bench(bench);
    // In the benchmark, a 1 ms timer measures the event loop's longest
    // stalls: a slow frame (or row creation) delays the next tick. Average
    // FPS hides such hitches.
    let stall_timer = slint::Timer::default();
    if bench {
        let mut last = std::time::Instant::now();
        let mut window_start = last;
        let mut worst = Duration::ZERO;
        let mut hitches = 0u32;
        stall_timer.start(slint::TimerMode::Repeated, Duration::from_millis(1), move || {
            let now = std::time::Instant::now();
            let gap = now - last;
            last = now;
            worst = worst.max(gap);
            if gap > Duration::from_millis(25) {
                hitches += 1;
            }
            if now - window_start >= Duration::from_secs(1) {
                eprintln!("stall: worst {} ms, hitches>25ms {hitches}", worst.as_millis());
                window_start = now;
                worst = Duration::ZERO;
                hitches = 0;
            }
        });
    }
    wire_login(&ui);
    wire_main(&ui);
    menus::wire(&ui);
    settings::wire(&ui);
    messages::wire(&ui);
    context_menu::wire(&ui);
    pictures::wire(&ui);

    // "Oxigram — 31 МБ ОЗУ", measured like the Tauri build does.
    let title_timer = slint::Timer::default();
    let weak = ui.as_weak();
    title_timer.start(slint::TimerMode::Repeated, TITLE_EVERY, move || {
        let Some(ui) = weak.upgrade() else { return };
        if let Some(info) = oxigram_core::mem::usage() {
            let unread: i32 = with_app(|app| app.unread_total()).unwrap_or(0);
            let count = if unread > 0 { format!(" ({unread})") } else { String::new() };
            ui.set_window_title(
                format!("Oxigram{count} — {} МБ ОЗУ", info.bytes / (1024 * 1024)).into(),
            );
        }
    });

    login_step(async { tg().state().await });
    ui.run().expect("event loop");
}

fn ui() -> Option<AppWindow> {
    with_app(|app| app.ui.upgrade()).flatten()
}

// ---- Login ---------------------------------------------------------------

fn wire_login(ui: &AppWindow) {
    ui.on_save_keys(|id, hash| {
        let id: i32 = id.trim().parse().unwrap_or(0);
        let hash = hash.to_string();
        login_step(async move { tg().save_keys(id, &hash).await });
    });
    ui.on_request_code(|phone| {
        let phone = phone.to_string();
        login_step(async move { tg().request_code(&phone).await });
    });
    ui.on_submit_code(|code| {
        let code = code.to_string();
        login_step(async move { tg().submit_code(&code).await });
    });
    ui.on_submit_password(|password| {
        let password = password.to_string();
        login_step(async move { tg().submit_password(&password).await });
    });
    ui.on_back_to_phone(|| login_step(async { tg().back_to_phone().await }));
    ui.on_change_keys(|| login_step(async { tg().clear_keys().await }));
}

fn login_step(step: impl Future<Output = Result<UiState, UiError>> + Send + 'static) {
    if let Some(ui) = ui() {
        ui.set_busy(true);
        ui.set_error(SharedString::new());
        ui.set_keys_rejected(false);
    }
    run(step, show_state);
}

fn show_state(result: Result<UiState, UiError>) {
    let Some(ui) = ui() else { return };
    ui.set_busy(false);
    let state = match result {
        Ok(state) => state,
        Err(err) => {
            ui.set_error(format::error(&err).into());
            let code = err.code.as_str();
            ui.set_keys_rejected(code == "API_ID_INVALID" || code == "API_ID_PUBLISHED_FLOOD");
            // The backend dropped the pending step: resync with it.
            if code == "WRONG_STAGE" || code == "PHONE_CODE_EXPIRED" {
                run(async { tg().state().await }, |state| {
                    if let Ok(state) = state {
                        show_state(Ok(state));
                    }
                });
            }
            if ui.get_stage() == "loading" {
                // No screen to keep yet (offline at start): retry shortly.
                slint::Timer::single_shot(Duration::from_secs(5), || {
                    login_step(async { tg().state().await });
                });
            }
            return;
        }
    };
    let stage = match state {
        UiState::NeedKeys => "keys",
        UiState::NeedPhone => "phone",
        UiState::NeedCode { phone } => {
            ui.set_login_phone(phone.into());
            "code"
        }
        UiState::NeedPassword { hint } => {
            ui.set_login_hint(hint.unwrap_or_default().into());
            "password"
        }
        UiState::Ready { me } => {
            menus::show_me(&me);
            if ui.get_stage() != "ready" {
                tg().ui.attach(Box::new(WindowSink));
                load_dialogs();
            }
            "ready"
        }
    };
    ui.set_stage(stage.into());
}

// ---- Dialog list ---------------------------------------------------------

fn wire_main(ui: &AppWindow) {
    ui.on_open_chat(|index| {
        let peer = with_app(|app| app.shown.get(index as usize).copied()).flatten();
        if let Some(peer) = peer {
            open_chat(peer);
        }
    });
    ui.on_filter_dialogs(|text| {
        with_app(|app| {
            app.filter = text.to_lowercase();
            app.refresh_dialogs();
        });
    });
    ui.on_toggle_archive(|| {
        with_app(|app| {
            app.show_archive = !app.show_archive;
            app.refresh_dialogs();
        });
    });
    ui.on_want_avatar(|index| {
        let peer = with_app(|app| app.shown.get(index as usize).copied()).flatten();
        if let Some(peer) = peer {
            request_avatar(peer);
        }
    });
    ui.on_load_more_dialogs(more_dialogs);
    ui.on_load_older(load_older);
    // Esc with nothing else open: hide the chat, as Telegram Desktop does.
    ui.on_close_chat(|| {
        pictures::close_viewer();
        with_app(|app| {
            app.chat = None;
            app.generation += 1;
            app.compose = messages::Compose::None;
            app.thumbs.clear();
            app.set_messages(Vec::new());
            if let Some(ui) = app.ui.upgrade() {
                ui.set_chat_open(false);
                ui.set_bar_mode(SharedString::new());
                ui.set_selecting_id(-1);
            }
            app.refresh_dialogs();
        });
        tg().ui.set_active_chat(None);
    });
    ui.on_toggle_mute(|| {
        if let Some(Some(peer)) = with_app(|app| app.chat) {
            menus::dialog_action(peer, "mute");
        }
    });
}

/// Shows the cached list at once, then the first page from Telegram.
fn load_dialogs() {
    run(async { chats::cached_dialogs(tg()).await }, |cached| {
        with_app(|app| {
            if let Ok(rows) = cached {
                app.dialogs = rows;
                app.sort_dialogs();
                app.refresh_dialogs();
            }
            app.dialogs_loading = true;
        });
        run(async { chats::load_dialogs(tg(), true).await }, |page| {
            with_app(|app| {
                app.dialogs_loading = false;
                match page {
                    Ok(page) => {
                        app.dialogs_done = page.done;
                        // Fresh rows replace their cached copies; cached chats
                        // further down stay until their page arrives.
                        let fresh: HashSet<i64> = page.dialogs.iter().map(|d| d.id).collect();
                        app.dialogs.retain(|d| !fresh.contains(&d.id));
                        let mut rows = page.dialogs;
                        rows.append(&mut app.dialogs);
                        app.dialogs = rows;
                        app.sort_dialogs();
                        app.refresh_dialogs();
                    }
                    Err(err) => app.chat_error(&err),
                }
            });
            prefetch_top();
            more_dialogs();
        });
    });
}

/// Fills the SQLite cache with the first page of the top chats, one after
/// another, so they open without waiting for the network. Read-only: it never
/// marks anything as read.
pub(crate) fn prefetch_top() {
    let peers = with_app(|app| {
        if !app.settings.prefetch_chats || app.prefetched || app.dialogs.is_empty() {
            return Vec::new();
        }
        app.prefetched = true;
        app.dialogs
            .iter()
            .filter(|d| !d.archived)
            .take(settings::PREFETCH_CHATS)
            .map(|d| d.id)
            .collect()
    })
    .unwrap_or_default();
    if peers.is_empty() {
        return;
    }
    RUNTIME.get().expect("set in main").spawn(async move {
        for peer in peers {
            let _ = chats::history(tg(), peer, None, settings::PREFETCH_PAGE).await;
        }
    });
}

fn more_dialogs() {
    let go = with_app(|app| {
        let go = !app.dialogs_loading && !app.dialogs_done;
        app.dialogs_loading |= go;
        go
    });
    if go != Some(true) {
        return;
    }
    run(async { chats::load_dialogs(tg(), false).await }, |page| {
        with_app(|app| {
            app.dialogs_loading = false;
            let Ok(page) = page else { return };
            app.dialogs_done = page.done;
            for row in page.dialogs {
                match app.dialogs.iter_mut().find(|d| d.id == row.id) {
                    Some(existing) => *existing = row,
                    None => app.dialogs.push(row),
                }
            }
            app.sort_dialogs();
            app.refresh_dialogs();
        });
        // The whole list (archive included) is small: keep paging in the
        // background so counts and search see every chat, not only the first page.
        let more = with_app(|app| !app.dialogs_done && app.dialogs.len() < MAX_DIALOGS);
        if more == Some(true) {
            slint::Timer::single_shot(Duration::from_millis(300), more_dialogs);
        }
    });
}

/// Fetches a chat photo (from the disk cache when it is there) and shrinks it
/// to the size it is drawn at.
fn request_avatar(peer: i64) {
    let Some(Some((photo_id, px))) = with_app(|app| {
        // "Фото на аватарках" off: nothing is fetched at all.
        if !app.settings.avatars || app.avatars.contains_key(&peer) || app.avatar_pending.contains(&peer) {
            return None;
        }
        let dialog = app.dialog(peer)?;
        if dialog.kind == "saved" {
            return None;
        }
        let photo_id = dialog.photo_id.clone()?;
        app.avatar_pending.insert(peer);
        let scale = app.ui.upgrade().map_or(1.0, |ui| ui.window().scale_factor());
        Some((photo_id, (AVATAR_PX * scale).ceil() as u32))
    }) else {
        return;
    };
    run(
        async move {
            let path = chats::avatar(tg(), peer, &photo_id).await.ok()?;
            tokio::task::spawn_blocking(move || {
                let picture = image::open(path).ok()?;
                let small = picture.resize_to_fill(px, px, image::imageops::FilterType::Triangle);
                Some(circle(small.to_rgba8()))
            })
            .await
            .ok()
            .flatten()
        },
        move |pixels| {
            with_app(|app| {
                // A failure stays "pending", so the photo is not retried in a loop.
                let Some(pixels) = pixels else { return };
                app.avatar_pending.remove(&peer);
                // Switched off while it was downloading.
                if !app.settings.avatars {
                    return;
                }
                let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                    pixels.as_raw(),
                    pixels.width(),
                    pixels.height(),
                );
                app.remember_avatar(peer, slint::Image::from_rgba8(buffer));
            });
        },
    );
}

/// Cuts the square photo to a circle with a soft edge. The software renderer
/// does not clip images to rounded corners, so the mask is in the pixels.
fn circle(mut pixels: image::RgbaImage) -> image::RgbaImage {
    let size = pixels.width() as f32;
    let radius = size / 2.0;
    for (x, y, pixel) in pixels.enumerate_pixels_mut() {
        let dx = x as f32 + 0.5 - radius;
        let dy = y as f32 + 0.5 - radius;
        let coverage = (radius - (dx * dx + dy * dy).sqrt() + 0.5).clamp(0.0, 1.0);
        pixel.0[3] = (pixel.0[3] as f32 * coverage).round() as u8;
    }
    pixels
}

// ---- Open chat -----------------------------------------------------------

fn open_chat(peer: i64) {
    pictures::close_viewer();
    let Some(generation) = with_app(|app| {
        app.chat = Some(peer);
        app.generation += 1;
        app.messages.clear();
        app.message_model.clear();
        app.thumbs.clear();
        app.history_done = false;
        app.history_loading = true;
        app.compose = messages::Compose::None;
        if let Some(ui) = app.ui.upgrade() {
            ui.set_bar_mode(SharedString::new());
            ui.set_selecting_id(-1);
            let (title, saved) = app
                .dialog(peer)
                .map(|d| (format::dialog_title(d), d.kind == "saved"))
                .unwrap_or_default();
            ui.set_chat_initial(format::initial(&title).into());
            ui.set_chat_title(title.into());
            ui.set_chat_saved(saved);
            ui.set_chat_color(format::avatar_color(peer));
            app.show_chat_photo(&ui);
            app.show_subtitle(&ui, None);
            ui.set_chat_error(SharedString::new());
            app.show_chat_rights(&ui);
            ui.set_chat_open(true);
            ui.set_history_loading(true);
            ui.invoke_set_composer(SharedString::new());
            ui.invoke_scroll_to_bottom();
        }
        app.refresh_dialogs();
        app.generation
    }) else {
        return;
    };
    tg().ui.set_active_chat(Some(peer));
    request_avatar(peer);

    // Member counts for groups and channels come with the full profile.
    let kind = with_app(|app| app.dialog(peer).map(|d| d.kind.clone())).flatten();
    if matches!(kind.as_deref(), Some("group" | "channel")) {
        run(async move { chats::peer_info(tg(), peer).await }, move |info| {
            with_app(|app| {
                if app.generation != generation {
                    return;
                }
                if let (Ok(info), Some(ui)) = (info, app.ui.upgrade()) {
                    app.show_subtitle(&ui, Some((info.members, info.online)));
                }
            });
        });
    }

    let unread = with_app(|app| app.dialog(peer).is_some_and(|d| d.unread > 0)).unwrap_or(false);
    run(
        async move { chats::cached_history(tg(), peer, None, HISTORY_PAGE).await },
        move |cached| {
            with_app(|app| {
                if app.generation != generation {
                    return;
                }
                if let Ok(mut rows) = cached {
                    rows.reverse();
                    app.set_messages(rows);
                }
            });
            run(
                async move { chats::history(tg(), peer, None, HISTORY_PAGE).await },
                move |fresh| {
                    with_app(|app| {
                        if app.generation != generation {
                            return;
                        }
                        app.history_loading = false;
                        match fresh {
                            Ok(mut rows) => {
                                app.history_done = rows.len() < HISTORY_PAGE;
                                rows.reverse();
                                app.set_messages(rows);
                            }
                            Err(err) => app.chat_error(&err),
                        }
                        if let Some(ui) = app.ui.upgrade() {
                            ui.set_history_loading(false);
                        }
                    });
                },
            );
        },
    );
    // Opening a chat reads it, as in the official apps.
    if unread {
        mark_read(peer);
    }
}

fn mark_read(peer: i64) {
    run(async move { chats::mark_read(tg(), peer).await }, move |done| {
        if done.is_ok() {
            with_app(|app| {
                if let Some(dialog) = app.dialogs.iter_mut().find(|d| d.id == peer) {
                    dialog.unread = 0;
                }
                app.refresh_dialogs();
            });
        }
    });
}

fn load_older() {
    let Some(Some((peer, before, generation))) = with_app(|app| {
        if app.history_loading || app.history_done {
            return None;
        }
        let peer = app.chat?;
        let before = app.messages.first()?.id;
        app.history_loading = true;
        if let Some(ui) = app.ui.upgrade() {
            ui.set_history_loading(true);
        }
        Some((peer, before, app.generation))
    }) else {
        return;
    };
    run(
        async move { chats::history(tg(), peer, Some(before), HISTORY_PAGE).await },
        move |page| {
            with_app(|app| {
                if app.generation != generation {
                    return;
                }
                app.history_loading = false;
                if let Some(ui) = app.ui.upgrade() {
                    ui.set_history_loading(false);
                }
                match page {
                    Ok(rows) => {
                        app.history_done = rows.len() < HISTORY_PAGE;
                        let started = std::time::Instant::now();
                        let count = rows.len();
                        app.prepend(rows);
                        if std::env::var_os("OXIGRAM_BENCH").is_some() {
                            eprintln!("prepend {count} rows: {} ms", started.elapsed().as_millis());
                        }
                    }
                    Err(err) => app.chat_error(&err),
                }
            });
        },
    );
}

impl App {
    fn dialog(&self, peer: i64) -> Option<&DialogRow> {
        self.dialogs.iter().find(|d| d.id == peer)
    }

    /// The header line under the chat title: presence for people, member
    /// counts for groups and channels once the profile has arrived.
    fn show_subtitle(&self, ui: &AppWindow, members: Option<(Option<i32>, Option<i32>)>) {
        let Some(dialog) = self.chat.and_then(|peer| self.dialog(peer)) else { return };
        let (text, accent) = match dialog.kind.as_str() {
            "user" if dialog.bot => ("бот".to_owned(), false),
            "user" => (
                format::presence_text(dialog.presence.as_ref()),
                format::is_online(dialog.presence.as_ref()),
            ),
            "channel" | "group" => {
                let (count, online) = members.unwrap_or_default();
                let channel = dialog.kind == "channel";
                let mut text = match count {
                    Some(n) if channel => {
                        format!("{n} {}", format::plural(n.into(), "подписчик", "подписчика", "подписчиков"))
                    }
                    Some(n) => format!("{n} {}", format::plural(n.into(), "участник", "участника", "участников")),
                    None if channel => "канал".to_owned(),
                    None => "группа".to_owned(),
                };
                if let Some(online) = online.filter(|n| *n > 0 && !channel) {
                    text.push_str(&format!(", {online} в сети"));
                }
                (text, false)
            }
            _ => (String::new(), false),
        };
        ui.set_chat_subtitle(text.into());
        ui.set_chat_subtitle_accent(accent);
    }

    fn show_chat_photo(&self, ui: &AppWindow) {
        match self.chat.and_then(|peer| self.avatars.get(&peer)) {
            Some(photo) => {
                ui.set_chat_photo(photo.clone());
                ui.set_chat_has_photo(true);
            }
            None => ui.set_chat_has_photo(false),
        }
    }

    fn remember_avatar(&mut self, peer: i64, photo: slint::Image) {
        self.avatars.insert(peer, photo.clone());
        self.avatar_order.retain(|p| *p != peer);
        self.avatar_order.push_back(peer);
        while self.avatar_order.len() > AVATAR_CACHE {
            if let Some(old) = self.avatar_order.pop_front() {
                self.avatars.remove(&old);
            }
        }
        for (index, id) in self.shown.iter().enumerate() {
            if *id == peer
                && let Some(mut item) = self.dialog_model.row_data(index)
            {
                item.photo = photo.clone();
                item.has_photo = true;
                self.dialog_model.set_row_data(index, item);
            }
        }
        if self.chat == Some(peer)
            && let Some(ui) = self.ui.upgrade()
        {
            self.show_chat_photo(&ui);
        }
    }

    fn unread_total(&self) -> i32 {
        self.dialogs
            .iter()
            .filter(|d| !d.muted && !d.archived)
            .map(|d| d.unread)
            .sum()
    }

    fn chat_error(&self, err: &UiError) {
        if let Some(ui) = self.ui.upgrade() {
            ui.set_chat_error(format::error(err).into());
        }
    }

    /// Pinned chats keep their order on top; the rest go newest first.
    fn sort_dialogs(&mut self) {
        self.dialogs
            .sort_by_key(|d| (!d.pinned, if d.pinned { 0 } else { -d.last_date }));
    }

    /// The list row for a dialog.
    pub(crate) fn dialog_item(&self, d: &DialogRow) -> DialogItem {
        let title = format::dialog_title(d);
        let saved = d.kind == "saved";
        let ticks = if d.last_outgoing && (d.kind == "user" || d.kind == "group") && d.last_id > 0 {
            if d.last_id <= d.read_outbox { 2 } else { 1 }
        } else {
            0
        };
        let photo = self.avatars.get(&d.id);
        DialogItem {
            initial: format::initial(&title).into(),
            title: title.into(),
            color: format::avatar_color(d.id),
            photo: photo.cloned().unwrap_or_default(),
            has_photo: photo.is_some(),
            preview: format::preview_text(&d.last_text).into(),
            you: d.last_outgoing && d.kind != "saved" && d.kind != "user",
            time: format::list_date(d.last_date).into(),
            unread: d.unread,
            muted: d.muted,
            pinned: d.pinned,
            online: d.kind == "user" && format::is_online(d.presence.as_ref()),
            ticks,
            saved,
            active: self.chat == Some(d.id),
            kind: d.kind.clone().into(),
            archived: d.archived,
        }
    }

    /// Composer or mute switch for the open chat, from its dialog row.
    fn show_chat_rights(&self, ui: &AppWindow) {
        let dialog = self.chat.and_then(|peer| self.dialog(peer));
        ui.set_chat_can_post(dialog.is_none_or(|d| d.can_post));
        ui.set_chat_muted(dialog.is_some_and(|d| d.muted));
    }

    fn refresh_dialogs(&mut self) {
        let needle = &self.filter;
        let archived = self.dialogs.iter().filter(|d| d.archived).count();
        // The archive opens as its own list; search looks everywhere.
        let rows: Vec<&DialogRow> = self
            .dialogs
            .iter()
            .filter(|d| if needle.is_empty() { d.archived == self.show_archive } else { true })
            .filter(|d| needle.is_empty() || format::dialog_title(d).to_lowercase().contains(needle))
            .collect();
        self.shown = rows.iter().map(|d| d.id).collect();
        let items: Vec<DialogItem> = rows.into_iter().map(|d| self.dialog_item(d)).collect();
        self.dialog_model.set_vec(items);
        if let Some(ui) = self.ui.upgrade() {
            self.show_chat_rights(&ui);
        }
        if let Some(ui) = self.ui.upgrade() {
            ui.set_show_archive(self.show_archive);
            ui.set_archived_count(archived as i32);
            let n = archived as i64;
            ui.set_archived_label(format!("{n} {}", format::plural(n, "чат", "чата", "чатов")).into());
        }
    }

    fn read_outbox(&self) -> i32 {
        self.chat
            .and_then(|peer| self.dialog(peer))
            .map(|d| d.read_outbox)
            .unwrap_or(0)
    }

    fn chat_kind(&self) -> &str {
        self.chat
            .and_then(|peer| self.dialog(peer))
            .map(|d| d.kind.as_str())
            .unwrap_or("")
    }

    /// Whether `b` continues the bubble run of `a` (same side, same day, and
    /// in groups the same sender within a few minutes).
    fn continues(&self, a: &MessageRow, b: &MessageRow) -> bool {
        if a.kind == "service" || b.kind == "service" || a.outgoing != b.outgoing {
            return false;
        }
        if !format::same_day(a.date, b.date) {
            return false;
        }
        if self.chat_kind() == "group" && !b.outgoing {
            return a.sender_id == b.sender_id && b.date - a.date <= RUN_GAP;
        }
        true
    }

    /// The row for `messages[index]`, which depends on its neighbours (date
    /// pill, sender name, gaps and the bubble tail).
    fn item(&self, index: usize) -> MessageItem {
        // An album is one bubble, drawn by its first message; the other
        // messages of the album keep empty rows.
        let album = self.album(index);
        if album.start != index {
            return MessageItem { id: self.messages[index].id, hidden: true, ..Default::default() };
        }
        let parts: Vec<&MessageRow> = self.messages[album.clone()].iter().collect();
        let media = pictures::layout(&parts);
        let m = &self.messages[index];
        let prev = index.checked_sub(1).map(|i| &self.messages[i]);
        let next = self.messages.get(album.end);
        let new_day = prev.is_none_or(|p| !format::same_day(p.date, m.date));
        let joined = prev.is_some_and(|p| self.continues(p, m));
        let tail = next.is_none_or(|n| !self.continues(m, n));
        let service = m.kind == "service";
        let saved = self.chat_kind() == "saved";
        let group = self.chat_kind() == "group";
        let ticks = if m.outgoing && !saved && !service && self.chat_kind() != "channel" {
            if m.id <= self.read_outbox() { 2 } else { 1 }
        } else {
            0
        };
        // The caption of an album may sit on any of its messages.
        let text = parts
            .iter()
            .map(|p| p.text.as_str())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        // A label only for what is not drawn as a picture (files, voice…).
        let label = parts
            .iter()
            .find(|p| !pictures::has_picture(p))
            .and_then(|p| format::kind_label(&p.kind))
            .unwrap_or_default();
        let media_only = media.is_some() && text.is_empty() && label.is_empty();
        MessageItem {
            id: m.id,
            multiline: text.contains('\n'),
            text: text.into(),
            media: label.into(),
            cells: media.as_ref().map(|l| self.media_cells(l)).unwrap_or_default(),
            media_w: media.as_ref().map_or(0.0, |l| l.w),
            media_h: media.as_ref().map_or(0.0, |l| l.h),
            media_only,
            sticker: media.as_ref().is_some_and(|l| l.sticker),
            round: media.as_ref().is_some_and(|l| l.round),
            hidden: false,
            time: format::time(m.date).into(),
            outgoing: m.outgoing,
            ticks,
            sender: if group && !m.outgoing && !joined && !service {
                m.sender_name.clone().into()
            } else {
                SharedString::new()
            },
            sender_color: format::name_color(m.sender_id.unwrap_or_default()),
            service,
            day: if new_day { format::day(m.date).into() } else { SharedString::new() },
            edited: m.edited,
            joined,
            tail,
            can_edit: m.outgoing && m.kind == "text",
        }
    }

    /// Recomputes a row after its neighbours changed.
    fn refresh_row(&self, index: usize) {
        if index < self.messages.len() {
            self.message_model.set_row_data(index, self.item(index));
            // The album's bubble lives on its first row.
            let head = self.album(index).start;
            if head != index {
                self.message_model.set_row_data(head, self.item(head));
            }
        }
    }

    fn set_messages(&mut self, rows: Vec<MessageRow>) {
        self.messages = rows;
        let items: Vec<MessageItem> = (0..self.messages.len()).map(|i| self.item(i)).collect();
        self.message_model.set_vec(items);
        if let Some(ui) = self.ui.upgrade() {
            ui.invoke_scroll_to_bottom();
        }
    }

    /// Older messages, newest first, go above what is shown.
    fn prepend(&mut self, rows: Vec<MessageRow>) {
        let known: HashSet<i32> = self.messages.iter().map(|m| m.id).collect();
        let mut older: Vec<MessageRow> = rows.into_iter().filter(|m| !known.contains(&m.id)).collect();
        if older.is_empty() {
            return;
        }
        older.reverse();
        let count = older.len();
        older.append(&mut self.messages);
        self.messages = older;
        for i in (0..count).rev() {
            self.message_model.insert(0, self.item(i));
        }
        // The first message shown before may lose its date pill or name.
        self.refresh_row(count);
    }

    /// A message sent from here or arriving live in the open chat.
    fn add_message(&mut self, row: MessageRow) {
        if self.chat != Some(row.peer_id) {
            return;
        }
        if let Some(index) = self.messages.iter().position(|m| m.id == row.id) {
            self.messages[index] = row;
            self.refresh_row(index);
            return;
        }
        self.messages.push(row);
        let index = self.messages.len() - 1;
        self.message_model.push(self.item(index));
        // The previous bubble may no longer be the last of its run.
        if let Some(prev) = index.checked_sub(1) {
            self.refresh_row(prev);
        }
        // Follow new messages only when the user is already at the bottom.
        if let Some(ui) = self.ui.upgrade()
            && ui.get_stick_bottom()
        {
            ui.invoke_scroll_to_bottom();
        }
    }

    fn apply_events(&mut self, events: Vec<UiEvent>) {
        let mut read_now = None;
        for event in events {
            match event {
                UiEvent::Message { message, peer } => {
                    let open = self.chat == Some(message.peer_id);
                    match self.dialogs.iter_mut().find(|d| d.id == message.peer_id) {
                        Some(dialog) => {
                            dialog.last_text = format::message_preview(&message);
                            dialog.last_date = message.date;
                            dialog.last_outgoing = message.outgoing;
                            dialog.last_id = message.id;
                            if !message.outgoing && !open {
                                dialog.unread += 1;
                            }
                        }
                        None => self.dialogs.push(DialogRow {
                            id: message.peer_id,
                            can_post: peer.kind != "channel",
                            kind: peer.kind,
                            title: peer.title,
                            last_text: format::message_preview(&message),
                            last_date: message.date,
                            last_outgoing: message.outgoing,
                            last_id: message.id,
                            unread: i32::from(!message.outgoing && !open),
                            pinned: false,
                            archived: false,
                            photo_id: peer.photo_id,
                            read_outbox: 0,
                            unread_reactions: 0,
                            muted: false,
                            contact: false,
                            bot: false,
                            presence: None,
                        }),
                    }
                    if open && !message.outgoing {
                        read_now = Some(message.peer_id);
                    }
                    self.add_message(message);
                }
                UiEvent::Edited { message } => {
                    if let Some(dialog) = self
                        .dialogs
                        .iter_mut()
                        .find(|d| d.id == message.peer_id && d.last_id == message.id)
                    {
                        dialog.last_text = format::message_preview(&message);
                    }
                    if self.messages.iter().any(|m| m.id == message.id) {
                        self.add_message(message);
                    }
                }
                UiEvent::Deleted { peer_id, ids } => {
                    let here = peer_id.is_none_or(|peer| self.chat == Some(peer));
                    if here {
                        while let Some(index) = self.messages.iter().position(|m| ids.contains(&m.id)) {
                            self.messages.remove(index);
                            self.message_model.remove(index);
                            if let Some(prev) = index.checked_sub(1) {
                                self.refresh_row(prev);
                            }
                            self.refresh_row(index);
                        }
                    }
                }
                UiEvent::ReadOutbox { peer_id, max_id } => {
                    if let Some(dialog) = self.dialogs.iter_mut().find(|d| d.id == peer_id) {
                        dialog.read_outbox = dialog.read_outbox.max(max_id);
                    }
                    if self.chat == Some(peer_id) {
                        for index in 0..self.messages.len() {
                            let m = &self.messages[index];
                            if m.outgoing && m.id <= max_id {
                                self.refresh_row(index);
                            }
                        }
                    }
                }
                UiEvent::Presence { peer_id, presence } => {
                    if let Some(dialog) = self.dialogs.iter_mut().find(|d| d.id == peer_id) {
                        dialog.presence = Some(presence);
                    }
                    if self.chat == Some(peer_id)
                        && let Some(ui) = self.ui.upgrade()
                    {
                        self.show_subtitle(&ui, None);
                    }
                }
                UiEvent::Connection { online } => {
                    if let Some(ui) = self.ui.upgrade() {
                        ui.set_online(online);
                    }
                }
                _ => {}
            }
        }
        self.sort_dialogs();
        self.refresh_dialogs();
        if let Some(peer) = read_now {
            mark_read(peer);
        }
    }
}
