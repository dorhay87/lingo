use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt as _, Shortcut};

use crate::config::ConfigError;

pub fn parse(hotkey: &str) -> Result<Shortcut, String> {
    hotkey.parse::<Shortcut>().map_err(|e| e.to_string())
}

pub fn register(app: &AppHandle, hotkey: &str) -> Result<(), ConfigError> {
    let shortcut = parse(hotkey).map_err(|_| ConfigError::InvalidHotkey(hotkey.into()))?;
    app.global_shortcut()
        .register(shortcut)
        .map_err(|_| ConfigError::HotkeyInUse)
}

pub fn unregister(app: &AppHandle, hotkey: &str) {
    if let Ok(shortcut) = parse(hotkey) {
        if let Err(e) = app.global_shortcut().unregister(shortcut) {
            log::warn!("failed to unregister '{hotkey}': {e}");
        }
    }
}

/// Register the new binding first; only drop the old one once the new one
/// holds, so a taken shortcut keeps the previous binding working.
pub fn remap(app: &AppHandle, old: &str, new: &str) -> Result<(), ConfigError> {
    register(app, new)?;
    unregister(app, old);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_hotkey() {
        assert!(parse("Ctrl+T").is_ok());
    }

    #[test]
    fn parses_multi_modifier_combos() {
        assert!(parse("Ctrl+Shift+Space").is_ok());
        assert!(parse("Alt+F9").is_ok());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse("NotAKey+definitely").is_err());
        assert!(parse("").is_err());
    }
}
