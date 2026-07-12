#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod capture;
mod commands;
mod config;
mod hotkey;
mod popup;
mod settings;
mod translate;
mod types;

use std::sync::{Arc, Mutex, RwLock};

use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::ShortcutState;

use capture::{SelectionCapture, SystemSelectionCapture};
use config::Config;
use types::Lang;

/// Per-popup-session state owned by Rust; the frontend only renders it.
#[derive(Default)]
pub struct PopupSession {
    pub src: Lang,
    pub tgt: Lang,
    pub pinned: bool,
    pub last_primary: Option<String>,
    pub last_detected: Option<Lang>,
}

pub struct AppState {
    pub config: RwLock<Config>,
    pub session: Mutex<PopupSession>,
    pub capture: Arc<dyn SelectionCapture>,
    pub http: reqwest::Client,
    pub orchestrator: translate::Orchestrator,
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItemBuilder::with_id("open", "Open translator").build(app)?;
    let settings = MenuItemBuilder::with_id("settings", "Settings").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
    let menu = MenuBuilder::new(app)
        .items(&[&open, &settings])
        .separator()
        .item(&quit)
        .build()?;

    let icon = app
        .default_window_icon()
        .cloned()
        .expect("bundled window icon missing");

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("Lingo")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            // Same path as the hotkey, minus selection capture.
            "open" => popup::show_at_cursor(app, false),
            "settings" => settings::open(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                popup::show_at_cursor(tray.app_handle(), false);
            }
        })
        .build(app)?;
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(if cfg!(debug_assertions) {
                    log::LevelFilter::Debug
                } else {
                    log::LevelFilter::Info
                })
                .build(),
        )
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    // Only the configured activation shortcut is ever registered.
                    if event.state() == ShortcutState::Pressed {
                        popup::toggle(app, true);
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::hide_popup,
            commands::popup_ready,
            commands::resize_popup,
            commands::translate,
            commands::set_langs,
            commands::swap_langs,
            commands::copy_result,
            commands::copy_text,
            commands::update_config,
            commands::open_settings,
            commands::pin_toggle,
            commands::speak,
        ])
        .setup(|app| {
            let config = config::load(app.handle());
            config::apply_theme(app.handle(), config.theme);
            if let Err(e) = hotkey::register(app.handle(), &config.hotkey) {
                log::error!("registering hotkey '{}' failed: {e}", config.hotkey);
            }
            app.manage(AppState {
                config: RwLock::new(config),
                session: Mutex::new(PopupSession::default()),
                capture: Arc::new(SystemSelectionCapture),
                http: reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(10))
                    .build()
                    .expect("http client"),
                orchestrator: translate::Orchestrator::default(),
            });
            build_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| match window.label() {
            "popup" => {
                if let WindowEvent::Focused(false) = event {
                    popup::hide_on_blur(window.app_handle());
                }
            }
            settings::LABEL => {
                // Hide, don't destroy: reopening from the tray stays instant.
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("error while running lingo");
}
