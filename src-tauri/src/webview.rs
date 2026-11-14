//! Keeps the WebView lean while nobody looks at it.
//!
//! When the window is minimized WebView2 is asked to drop to its "low" memory
//! target: it trims caches and pages out what it can, which on a typical chat
//! list frees most of the renderer's working set. Restoring the window brings
//! it back to normal. Other platforms have no such switch and do nothing.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{Manager, Runtime, Window};

/// Whether the low target is currently set, so repeated resize events while
/// minimized cost nothing.
static LOW: AtomicBool = AtomicBool::new(false);

/// A destroyed window takes its WebView with it, and the next one starts at the
/// normal level, so the flag must not stay set.
pub fn on_destroyed() {
    LOW.store(false, Ordering::Relaxed);
}

pub fn on_resized<R: Runtime>(window: &Window<R>) {
    let minimized = window.is_minimized().unwrap_or(false);
    if LOW.swap(minimized, Ordering::Relaxed) == minimized {
        return;
    }
    if let Some(webview) = window.app_handle().get_webview_window(window.label()) {
        set_low_memory(&webview, minimized);
    }
}

#[cfg(windows)]
fn set_low_memory<R: Runtime>(webview: &tauri::WebviewWindow<R>, low: bool) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
        ICoreWebView2_19,
    };
    use windows_core::Interface;

    let _ = webview.with_webview(move |platform| {
        // SAFETY: COM calls on the WebView2 controller, made on the UI thread
        // that `with_webview` runs this closure on.
        unsafe {
            let Ok(core) = platform.controller().CoreWebView2() else {
                return;
            };
            // Older WebView2 runtimes lack the interface; they simply keep
            // their memory.
            let Ok(core) = core.cast::<ICoreWebView2_19>() else {
                return;
            };
            let level = if low {
                COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
            } else {
                COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
            };
            let _ = core.SetMemoryUsageTargetLevel(level);
        }
    });
}

#[cfg(not(windows))]
fn set_low_memory<R: Runtime>(_webview: &tauri::WebviewWindow<R>, _low: bool) {}
