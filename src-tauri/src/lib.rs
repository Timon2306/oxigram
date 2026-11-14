mod commands;
mod title;
mod tray;
mod webview;

// The backend lives in `oxigram-core`, shared with the native build.
use oxigram_core::{actions, chats, error, store, tg, updates};

/// Live events travel to the WebView over a Tauri channel.
struct ChannelSink(tauri::ipc::Channel<Vec<updates::UiEvent>>);

impl updates::EventSink for ChannelSink {
    fn send(&self, events: Vec<updates::UiEvent>) -> bool {
        self.0.send(events).is_ok()
    }
}

/// System notifications through the Tauri notification plugin.
struct TauriNotifier(tauri::AppHandle);

impl updates::Notifier for TauriNotifier {
    fn looking(&self) -> bool {
        self.0.get_webview_window(tray::MAIN).is_some_and(|window| {
            window.is_focused().unwrap_or(false) && !window.is_minimized().unwrap_or(true)
        })
    }

    fn show(&self, title: String, body: String) {
        use tauri_plugin_notification::NotificationExt;
        let _ = self.0.notification().builder().title(title).body(body).show();
    }
}

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // A second copy would share the database and the session with the
        // first; starting Oxigram again just brings the running one forward.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        // Started with Windows, the app opens straight to the tray with no
        // window (and no WebView): about 5 MB until the user opens it.
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![tray::MINIMIZED_ARG]),
        ))
        .setup(|app| {
            let paths = app.path();
            app.manage(tg::Telegram::new(
                paths.app_config_dir()?,
                paths.app_data_dir()?,
                paths.app_cache_dir()?.join("media"),
            ));
            app.state::<tg::Telegram>()
                .ui
                .set_notifier(Box::new(TauriNotifier(app.handle().clone())));
            // Connect right away, not when the window first asks: started into
            // the tray there is no window, yet new messages should still ring.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let _ = handle.state::<tg::Telegram>().state().await;
            });
            title::show_in_title(app.handle().clone());
            // A missing tray is not fatal: the app then quits when closed.
            let _ = tray::init(app.handle());
            // The window is created hidden (tauri.conf.json) and shown here,
            // unless the app was started into the tray.
            if let Some(window) = app.get_webview_window("main") {
                let minimized = std::env::args().any(|arg| arg == tray::MINIMIZED_ARG);
                if minimized && tray::available() {
                    let _ = window.destroy();
                } else {
                    let _ = window.show();
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::Resized(_) => webview::on_resized(window),
            tauri::WindowEvent::Destroyed => webview::on_destroyed(),
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::save_keys,
            commands::clear_keys,
            commands::request_code,
            commands::submit_code,
            commands::submit_password,
            commands::back_to_phone,
            commands::sign_out,
            commands::cached_dialogs,
            commands::load_dialogs,
            commands::cached_history,
            commands::history,
            commands::send_message,
            commands::mark_read,
            commands::message_image,
            commands::avatar,
            commands::custom_emoji,
            commands::message_file,
            commands::save_file,
            commands::peer_info,
            commands::peer_presence,
            commands::shared_media,
            commands::set_muted,
            commands::contacts,
            commands::set_active_chat,
            commands::set_unread,
            commands::send_file,
            commands::upload_file,
            commands::send_album,
            commands::set_typing,
            commands::folders,
            commands::cache_size,
            commands::clear_cache,
            commands::search_chat,
            commands::search_global,
            commands::resolve_username,
            commands::pin_dialog,
            commands::mark_unread,
            commands::archive_dialog,
            commands::clear_history,
            commands::delete_chat,
            commands::react,
            commands::allowed_reactions,
            commands::edit_message,
            commands::delete_messages,
            commands::pin_message,
            commands::forward_messages,
            commands::subscribe,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            // Closing the last window asks to exit with no code: keep running
            // in the tray instead. `app.exit(..)` carries a code and goes through.
            // Without a tray icon (some Linux desktops) there would be no way
            // back, so the app simply quits.
            if let tauri::RunEvent::ExitRequested { code: None, api, .. } = event {
                if tray::available() {
                    api.prevent_exit();
                }
            }
        });
}
