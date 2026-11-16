//! "Экономия памяти": what costs memory and traffic, each switchable. Kept in
//! `native-settings.json` next to the keys, but never inside `config.json`,
//! which holds the API keys and is not ours to rewrite.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use slint::{ModelRc, VecModel};

use crate::{AppWindow, SaverOption, menus, ui, with_app};

const FILE: &str = "native-settings.json";
/// How many chats from the top get their history fetched ahead of time.
pub const PREFETCH_CHATS: usize = 10;
/// Matches the first page a chat loads when opened.
pub const PREFETCH_PAGE: usize = 50;

static DIR: OnceLock<PathBuf> = OnceLock::new();

/// Remembers the config folder; call once at startup.
pub fn set_dir(dir: PathBuf) {
    let _ = DIR.set(dir);
}

/// The config folder, once `set_dir` has run.
pub fn dir() -> Option<&'static Path> {
    DIR.get().map(PathBuf::as_path)
}

/// Same keys and defaults as the WebView client's settings.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub avatars: bool,
    #[serde(rename = "autoloadImages")]
    pub autoload_images: bool,
    #[serde(rename = "autoplayGifs")]
    pub autoplay_gifs: bool,
    #[serde(rename = "customEmoji")]
    pub custom_emoji: bool,
    #[serde(rename = "prefetchChats")]
    pub prefetch_chats: bool,
    /// OpenGL renderer instead of the software one; read once at start-up.
    #[serde(rename = "gpuRender")]
    pub gpu_render: bool,
    /// "Выключить все уведомления": no system notifications at all.
    #[serde(rename = "muteAll")]
    pub mute_all: bool,
    /// Start with the system, hidden in the tray.
    pub autostart: bool,
    /// The light look (as in the WebView build); dark by default.
    #[serde(rename = "lightTheme")]
    pub light_theme: bool,
    /// "small", "normal" or "large" message text.
    #[serde(rename = "textSize")]
    pub text_size: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            avatars: true,
            autoload_images: true,
            autoplay_gifs: true,
            custom_emoji: false,
            prefetch_chats: true,
            gpu_render: false,
            mute_all: false,
            autostart: false,
            light_theme: false,
            text_size: "normal".to_owned(),
        }
    }
}

impl Settings {
    /// Defaults when the file is missing or unreadable.
    pub fn load_from(dir: &Path) -> Self {
        std::fs::read_to_string(dir.join(FILE))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save_to(&self, dir: &Path) {
        if let Ok(text) = serde_json::to_string_pretty(self) {
            // Kept for this session only if the disk refuses.
            let _ = std::fs::write(dir.join(FILE), text);
        }
    }

    pub fn load() -> Self {
        DIR.get().map(|dir| Self::load_from(dir)).unwrap_or_default()
    }

    pub fn save(&self) {
        if let Some(dir) = DIR.get() {
            self.save_to(dir);
        }
    }

    fn flag(&mut self, key: &str) -> Option<&mut bool> {
        Some(match key {
            "avatars" => &mut self.avatars,
            "autoloadImages" => &mut self.autoload_images,
            "autoplayGifs" => &mut self.autoplay_gifs,
            "customEmoji" => &mut self.custom_emoji,
            "prefetchChats" => &mut self.prefetch_chats,
            "gpuRender" => &mut self.gpu_render,
            "muteAll" => &mut self.mute_all,
            "autostart" => &mut self.autostart,
            "lightTheme" => &mut self.light_theme,
            _ => return None,
        })
    }

    /// The rows of the saver window.
    pub fn options(&self) -> Vec<SaverOption> {
        let mut copy = self.clone();
        let rows = [
            ("avatars", "Фото на аватарках", "Без них — цветной кружок с буквой."),
            (
                "autoloadImages",
                "Загружать картинки сами",
                "Без этого картинка и превью видео грузятся по клику. Заработает вместе с картинками.",
            ),
            (
                "autoplayGifs",
                "Проигрывать GIF и стикеры",
                "Без этого они стоят на месте, пока не откроешь. Заработает вместе с картинками.",
            ),
            (
                "customEmoji",
                "Кастомные эмодзи в реакциях",
                "Картинки таких реакций, по клику можно поставить такую же. Без этого — одна ✨ с общим числом. Заработает вместе с реакциями.",
            ),
            (
                "prefetchChats",
                "Подгружать верхние чаты",
                "Десять верхних чатов открываются сразу, без ожидания сети.",
            ),
            (
                "gpuRender",
                "Отрисовка на видеокарте",
                "Плавнее прокрутка и цветные эмодзи, но OpenGL-драйвер добавляет около 75 МБ ОЗУ. Применится после перезапуска.",
            ),
            (
                "muteAll",
                "Выключить все уведомления",
                "Системные уведомления о новых сообщениях не показываются.",
            ),
            (
                "autostart",
                "Запускать вместе с системой",
                "Oxigram стартует при входе в систему и ждёт в трее, окно не открывается.",
            ),
            ("lightTheme", "Светлая тема", "Светлое оформление вместо тёмного."),
        ];
        rows.into_iter()
            .map(|(key, title, note)| SaverOption {
                on: copy.flag(key).is_some_and(|f| *f),
                key: key.into(),
                title: title.into(),
                group: group(key).into(),
                // About 34 characters fit a line of the settings column.
                note_lines: (note.chars().count() as i32 + 33) / 34,
                note: note.into(),
            })
            .collect()
    }
}

/// The settings page an option is on.
fn group(key: &str) -> &'static str {
    match key {
        "muteAll" => "notifications",
        "gpuRender" | "autostart" | "lightTheme" => "general",
        _ => "data",
    }
}

pub fn wire(window: &AppWindow) {
    window.on_cycle_text_size(cycle_text_size);
    apply_look(window);
    // "Экономия памяти" in the burger menu opens the settings on that page.
    window.on_open_saver(|| {
        if let Some(ui) = ui() {
            fill(&ui);
            ui.set_cache_text(crate::menus::format_size(crate::tg().cache_size()).into());
            ui.set_settings_page("data".into());
            ui.set_settings_open(true);
        }
    });
    window.on_saver_toggle(|key| {
        let Some(Some(on)) = with_app(|app| {
            let flag = app.settings.flag(&key)?;
            *flag = !*flag;
            let on = *flag;
            app.settings.save();
            Some(on)
        }) else {
            return;
        };
        if let Some(ui) = ui() {
            fill(&ui);
        }
        match key.as_str() {
            "avatars" => apply_avatars(on),
            "muteAll" => crate::notify::set_mute_all(on),
            "autostart" => crate::autostart::set(on),
            "prefetchChats" if on => crate::prefetch_top(),
            "lightTheme" => {
                if let Some(ui) = ui() {
                    apply_look(&ui);
                }
            }
            // Custom emoji reactions: pictures, or one ✨ for all of them.
            "customEmoji" => {
                with_app(|app| {
                    if !on {
                        crate::reactions::drop_pictures(app);
                    }
                    app.refresh_all_rows();
                });
            }
            // Cells waiting for a click start loading, or the other way round.
            "autoloadImages" => {
                with_app(|app| app.refresh_all_rows());
            }
            _ => {}
        }
    });
}

/// The message text scale of a "Размер текста" choice (as in the WebView build).
fn scale(size: &str) -> f32 {
    match size {
        "small" => 0.9,
        "large" => 1.12,
        _ => 1.0,
    }
}

fn size_label(size: &str) -> &'static str {
    match size {
        "small" => "Мелкий",
        "large" => "Крупный",
        _ => "Обычный",
    }
}

/// Puts the theme and the text size from the settings on screen.
pub fn apply_look(ui: &AppWindow) {
    use slint::ComponentHandle;
    let Some((light, size)) = with_app(|app| (app.settings.light_theme, app.settings.text_size.clone())) else {
        return;
    };
    let theme = ui.global::<crate::Theme>();
    theme.set_light(light);
    theme.set_scale(scale(&size));
    ui.set_text_size_label(size_label(&size).into());
}

/// "Размер текста" in the settings: normal → large → small → normal.
fn cycle_text_size() {
    with_app(|app| {
        let next = match app.settings.text_size.as_str() {
            "normal" => "large",
            "large" => "small",
            _ => "normal",
        };
        app.settings.text_size = next.to_owned();
        app.settings.save();
    });
    if let Some(ui) = ui() {
        apply_look(&ui);
    }
}

pub(crate) fn fill(ui: &AppWindow) {
    let options = with_app(|app| app.settings.options()).unwrap_or_default();
    ui.set_saver_options(ModelRc::new(VecModel::from(options)));
}

/// Off drops every decoded photo; on lets the rows ask for theirs again.
fn apply_avatars(on: bool) {
    with_app(|app| {
        if !on {
            app.avatars.clear();
            app.avatar_order.clear();
            app.avatar_pending.clear();
        }
        // Rebuilding the model re-creates the visible rows, which ask for
        // their photos again through `want-avatar`.
        app.refresh_dialogs();
        if let Some(ui) = app.ui.upgrade() {
            app.show_chat_photo(&ui);
            if !on {
                ui.set_me_has_photo(false);
            }
        }
    });
    if on {
        menus::reload_me_photo();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oxigram-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_file_gives_defaults() {
        let dir = temp("missing");
        assert_eq!(Settings::load_from(&dir), Settings::default());
    }

    #[test]
    fn garbled_file_gives_defaults() {
        let dir = temp("garbled");
        std::fs::write(dir.join(FILE), "{ not json").unwrap();
        assert_eq!(Settings::load_from(&dir), Settings::default());
    }

    #[test]
    fn unknown_and_missing_fields_are_tolerated() {
        let dir = temp("partial");
        std::fs::write(dir.join(FILE), r#"{"avatars": false, "future": 1}"#).unwrap();
        let loaded = Settings::load_from(&dir);
        assert!(!loaded.avatars);
        assert!(loaded.prefetch_chats && !loaded.custom_emoji);
    }

    #[test]
    fn round_trip() {
        let dir = temp("round");
        let mut s = Settings::default();
        s.avatars = false;
        s.custom_emoji = true;
        s.save_to(&dir);
        assert_eq!(Settings::load_from(&dir), s);
    }

    #[test]
    fn gpu_render_is_off_by_default() {
        assert!(!Settings::default().gpu_render);
        let dir = temp("gpu");
        std::fs::write(dir.join(FILE), r#"{"gpuRender": true}"#).unwrap();
        assert!(Settings::load_from(&dir).gpu_render);
    }

    #[test]
    fn every_option_maps_to_a_flag() {
        let mut s = Settings::default();
        for option in s.options() {
            assert!(s.flag(&option.key).is_some());
        }
    }
}
