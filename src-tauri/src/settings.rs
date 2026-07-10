//! Settings window: lazily created on first open, hidden (not destroyed) on
//! close so reopening is instant.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::types::Theme;

pub const LABEL: &str = "settings";

pub fn open(app: &AppHandle) {
    // Settings replaces the popup on screen.
    crate::popup::hide(app);

    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        return;
    }

    let theme = {
        let state = app.state::<crate::AppState>();
        let config = state.config.read().expect("config lock poisoned");
        match config.theme {
            Theme::System => None,
            Theme::Light => Some(tauri::Theme::Light),
            Theme::Dark => Some(tauri::Theme::Dark),
        }
    };

    // Window creation must not run on the main thread's event loop: build()
    // blocks waiting for the loop to process the creation, so calling it from
    // a synchronous command or the tray handler deadlocks on Windows and
    // leaves an empty, unregistered husk window. Build from a worker thread,
    // where blocking on the (free) main loop is safe.
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result =
            WebviewWindowBuilder::new(&app, LABEL, WebviewUrl::App("settings.html".into()))
                .title("Lingo · Settings")
                .inner_size(520.0, 560.0)
                .min_inner_size(480.0, 400.0)
                .center()
                .theme(theme)
                .build();
        if let Err(e) = result {
            log::error!("failed to create settings window: {e}");
        }
    });
}
