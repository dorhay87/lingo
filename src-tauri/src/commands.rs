//! Thin #[tauri::command] wrappers; all logic lives in the service modules.

use tauri::{AppHandle, Emitter, State};
use tauri_plugin_clipboard_manager::ClipboardExt as _;

use crate::config::Config;
use crate::types::{langs_equal, Lang};
use crate::{popup, AppState};

#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> Config {
    state.config.read().expect("config lock poisoned").clone()
}

/// Esc always hides, pinned or not; blur is handled Rust-side.
#[tauri::command]
pub fn hide_popup(app: AppHandle) {
    popup::hide(&app);
}

/// Show-handshake completion: the popup has painted the state for the
/// current popup:show and is ready to become visible.
#[tauri::command]
pub fn popup_ready(app: AppHandle) {
    popup::reveal(&app);
}

#[tauri::command]
pub fn resize_popup(app: AppHandle, height: f64) {
    popup::resize(&app, height);
}

fn set_session_langs(state: &AppState, src: Lang, tgt: Lang) {
    let mut session = state.session.lock().expect("session lock poisoned");
    session.src = src;
    session.tgt = tgt;
}

/// Returns the monotonic request_id; the matching result/error arrives as a
/// broadcast event carrying the same id.
#[tauri::command]
pub fn translate(
    app: AppHandle,
    state: State<'_, AppState>,
    text: String,
    src: Lang,
    tgt: Lang,
) -> u64 {
    set_session_langs(&state, src.clone(), tgt.clone());
    state.orchestrator.submit(&app, text, src, tgt)
}

#[tauri::command]
pub fn set_langs(state: State<'_, AppState>, src: Lang, tgt: Lang) {
    set_session_langs(&state, src, tgt);
}

/// Swap source and target. An auto source resolves to the last detected
/// language; the decision stays in Rust and reaches the frontend as an event.
#[tauri::command]
pub fn swap_langs(app: AppHandle, state: State<'_, AppState>) {
    let (src, tgt) = {
        let mut session = state.session.lock().expect("session lock poisoned");
        let resolved_src = if session.src == "auto" {
            match &session.last_detected {
                Some(detected) if !langs_equal(detected, &session.tgt) => detected.clone(),
                _ => return, // nothing meaningful to swap yet
            }
        } else {
            session.src.clone()
        };
        let new_src = session.tgt.clone();
        session.src = new_src.clone();
        session.tgt = resolved_src.clone();
        (new_src, resolved_src)
    };
    let _ = app.emit("popup:langs", serde_json::json!({ "src": src, "tgt": tgt }));
}

/// Enter: copy the primary translation and dismiss.
#[tauri::command]
pub fn copy_result(app: AppHandle, state: State<'_, AppState>) {
    let text = state
        .session
        .lock()
        .expect("session lock poisoned")
        .last_primary
        .clone();
    if let Some(text) = text {
        if let Err(e) = app.clipboard().write_text(text) {
            log::error!("clipboard write failed: {e}");
            return;
        }
        popup::hide(&app);
    }
}

/// Tapping a dictionary alternative: copy that term and dismiss.
#[tauri::command]
pub fn copy_text(app: AppHandle, text: String) {
    if let Err(e) = app.clipboard().write_text(text) {
        log::error!("clipboard write failed: {e}");
        return;
    }
    popup::hide(&app);
}

/// Pinned popups survive focus loss; Esc still hides. Pin state resets on
/// every popup:show.
#[tauri::command]
pub fn pin_toggle(app: AppHandle, state: State<'_, AppState>) {
    let pinned = {
        let mut session = state.session.lock().expect("session lock poisoned");
        session.pinned = !session.pinned;
        session.pinned
    };
    let _ = app.emit("popup:pin_changed", serde_json::json!({ "pinned": pinned }));
}

/// Single mutation path for config: validate, apply side effects (hotkey
/// re-registration, autostart, theme), persist, broadcast config:changed.
#[tauri::command]
pub fn update_config(
    app: AppHandle,
    patch: serde_json::Value,
) -> Result<(), crate::config::ConfigError> {
    crate::config::update(&app, patch)
}

#[tauri::command]
pub fn open_settings(app: AppHandle) {
    crate::settings::open(&app);
}

/// Settings "Test" button: run a one-word translation through the given
/// provider/key combination without touching the saved config.
#[tauri::command]
pub async fn test_provider(
    state: State<'_, AppState>,
    provider: crate::types::ProviderKind,
    api_key: String,
) -> Result<(), crate::types::ProviderError> {
    use crate::types::{ProviderKind, TranslateRequest};

    let mut config = state.config.read().expect("config lock poisoned").clone();
    config.provider = provider;
    if provider != ProviderKind::GoogleFree {
        config.api_keys.insert(provider, api_key);
    }
    let candidate = crate::translate::provider_for(&config, state.http.clone());
    // Pick a target the provider claims to support.
    let langs = candidate.supported_langs();
    let tgt = langs
        .iter()
        .find(|l| **l == "nl")
        .or_else(|| langs.first())
        .copied()
        .unwrap_or("en");
    candidate
        .translate(&TranslateRequest {
            request_id: 0,
            text: "hello".into(),
            src: "en".into(),
            tgt: tgt.into(),
            want_dictionary: false,
        })
        .await
        .map(|_| ())
}
