use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_store::StoreExt as _;

use crate::hotkey;
use crate::types::{langs_equal, Lang, ProviderKind, Theme};

const STORE_FILE: &str = "config.json";
const STORE_KEY: &str = "config";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub hotkey: String,
    pub provider: ProviderKind,
    pub api_keys: HashMap<ProviderKind, String>,
    pub source_lang: Lang,
    pub target_lang: Lang,
    pub lang_preferences: Vec<Lang>,
    pub launch_at_startup: bool,
    pub theme: Theme,
    /// Hex accent color driving --accent-base in both windows.
    pub accent: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: "Ctrl+L".into(),
            provider: ProviderKind::GoogleFree,
            api_keys: HashMap::new(),
            source_lang: "auto".into(),
            target_lang: "en".into(),
            lang_preferences: vec!["en".into(), "he".into(), "nl".into()],
            launch_at_startup: false,
            theme: Theme::System,
            accent: "#4F46E5".into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("shortcut is already in use by another app")]
    HotkeyInUse,
    #[error("'{0}' is not a valid shortcut")]
    InvalidHotkey(String),
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Io(String),
}

impl ConfigError {
    pub fn kind(&self) -> &'static str {
        match self {
            ConfigError::HotkeyInUse => "hotkey_in_use",
            ConfigError::InvalidHotkey(_) => "invalid_hotkey",
            ConfigError::Invalid(_) => "invalid_config",
            ConfigError::Io(_) => "io",
        }
    }
}

impl Serialize for ConfigError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("ConfigError", 2)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

fn is_hex_color(s: &str) -> bool {
    let Some(hex) = s.strip_prefix('#') else {
        return false;
    };
    matches!(hex.len(), 3 | 6) && hex.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn validate(config: &Config) -> Result<(), ConfigError> {
    hotkey::parse(&config.hotkey)
        .map_err(|_| ConfigError::InvalidHotkey(config.hotkey.clone()))?;
    if config.target_lang.trim().is_empty() {
        return Err(ConfigError::Invalid("target language is empty".into()));
    }
    if config.source_lang.trim().is_empty() {
        return Err(ConfigError::Invalid("source language is empty".into()));
    }
    if config.source_lang != "auto" && langs_equal(&config.source_lang, &config.target_lang) {
        return Err(ConfigError::Invalid(
            "source and target languages are the same".into(),
        ));
    }
    if config.lang_preferences.is_empty() {
        return Err(ConfigError::Invalid(
            "language preference list is empty".into(),
        ));
    }
    if config.lang_preferences.iter().any(|l| l.trim().is_empty()) {
        return Err(ConfigError::Invalid(
            "language preference list contains an empty code".into(),
        ));
    }
    let mut seen = std::collections::HashSet::new();
    if !config
        .lang_preferences
        .iter()
        .all(|l| seen.insert(l.to_ascii_lowercase()))
    {
        return Err(ConfigError::Invalid(
            "language preference list contains duplicates".into(),
        ));
    }
    if !is_hex_color(&config.accent) {
        return Err(ConfigError::Invalid(format!(
            "'{}' is not a hex color",
            config.accent
        )));
    }
    Ok(())
}

/// Load config from the JSON store, falling back to defaults on missing or
/// unreadable data. Never fails: a broken config file must not brick the app.
pub fn load(app: &AppHandle) -> Config {
    let Ok(store) = app.store(STORE_FILE) else {
        return Config::default();
    };
    match store.get(STORE_KEY) {
        Some(value) => match serde_json::from_value::<Config>(value) {
            Ok(config) if validate(&config).is_ok() => config,
            Ok(_) | Err(_) => {
                log::warn!("stored config invalid, using defaults");
                Config::default()
            }
        },
        None => Config::default(),
    }
}

pub fn persist(app: &AppHandle, config: &Config) -> Result<(), ConfigError> {
    let store = app
        .store(STORE_FILE)
        .map_err(|e| ConfigError::Io(e.to_string()))?;
    store.set(
        STORE_KEY,
        serde_json::to_value(config).map_err(|e| ConfigError::Io(e.to_string()))?,
    );
    store.save().map_err(|e| ConfigError::Io(e.to_string()))?;
    Ok(())
}

/// Shallow-merge a JSON patch onto the current config. Top-level fields
/// replace wholesale; unknown fields are rejected by deserialization below.
pub fn merge_patch(current: &Config, patch: &serde_json::Value) -> Result<Config, ConfigError> {
    let mut value =
        serde_json::to_value(current).map_err(|e| ConfigError::Io(e.to_string()))?;
    let (Some(obj), Some(patch_obj)) = (value.as_object_mut(), patch.as_object()) else {
        return Err(ConfigError::Invalid("patch must be an object".into()));
    };
    for (k, v) in patch_obj {
        if !obj.contains_key(k) {
            return Err(ConfigError::Invalid(format!("unknown config field '{k}'")));
        }
        obj.insert(k.clone(), v.clone());
    }
    serde_json::from_value(value).map_err(|e| ConfigError::Invalid(e.to_string()))
}

pub fn apply_theme(app: &AppHandle, theme: Theme) {
    let tauri_theme = match theme {
        Theme::System => None,
        Theme::Light => Some(tauri::Theme::Light),
        Theme::Dark => Some(tauri::Theme::Dark),
    };
    for window in app.webview_windows().values() {
        let _ = window.set_theme(tauri_theme);
    }
}

/// Validate a patched config, apply side effects (hotkey re-registration,
/// autostart, theme), persist, update state, and broadcast config:changed.
/// The hotkey swap happens first and aborts the whole update on failure so a
/// taken shortcut never clobbers the working binding.
pub fn update(app: &AppHandle, patch: serde_json::Value) -> Result<(), ConfigError> {
    let state = app.state::<crate::AppState>();
    let old = state.config.read().expect("config lock poisoned").clone();
    let new = merge_patch(&old, &patch)?;
    validate(&new)?;

    if new.hotkey != old.hotkey {
        hotkey::remap(app, &old.hotkey, &new.hotkey)?;
    }

    if new.launch_at_startup != old.launch_at_startup {
        let autolaunch = app.autolaunch();
        let result = if new.launch_at_startup {
            autolaunch.enable()
        } else {
            autolaunch.disable()
        };
        if let Err(e) = result {
            log::error!("autostart toggle failed: {e}");
        }
    }

    if new.theme != old.theme {
        apply_theme(app, new.theme);
    }

    persist(app, &new)?;
    *state.config.write().expect("config lock poisoned") = new.clone();
    let _ = app.emit("config:changed", &new);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_are_valid() {
        validate(&Config::default()).unwrap();
    }

    #[test]
    fn default_config_values() {
        let c = Config::default();
        assert_eq!(c.hotkey, "Ctrl+L");
        assert_eq!(c.provider, ProviderKind::GoogleFree);
        assert_eq!(c.target_lang, "en");
        assert_eq!(c.lang_preferences, vec!["en", "he", "nl"]);
        assert!(!c.launch_at_startup);
        assert_eq!(c.theme, Theme::System);
    }

    #[test]
    fn deserializes_partial_json_with_defaults() {
        let c: Config = serde_json::from_value(json!({ "target_lang": "nl" })).unwrap();
        assert_eq!(c.target_lang, "nl");
        assert_eq!(c.hotkey, "Ctrl+L");
    }

    #[test]
    fn merge_patch_replaces_top_level_fields() {
        let base = Config::default();
        let patched = merge_patch(
            &base,
            &json!({ "provider": "DeepL", "api_keys": { "DeepL": "k:fx" } }),
        )
        .unwrap();
        assert_eq!(patched.provider, ProviderKind::DeepL);
        assert_eq!(
            patched.api_keys.get(&ProviderKind::DeepL).unwrap(),
            "k:fx"
        );
        assert_eq!(patched.hotkey, base.hotkey);
    }

    #[test]
    fn merge_patch_rejects_unknown_fields() {
        assert!(matches!(
            merge_patch(&Config::default(), &json!({ "nope": 1 })),
            Err(ConfigError::Invalid(_))
        ));
    }

    fn config_with(mutate: impl FnOnce(&mut Config)) -> Config {
        let mut config = Config::default();
        mutate(&mut config);
        config
    }

    #[test]
    fn validate_rejects_bad_hotkey() {
        let c = config_with(|c| c.hotkey = "NotAKey+definitely".into());
        assert!(matches!(validate(&c), Err(ConfigError::InvalidHotkey(_))));
    }

    #[test]
    fn validate_rejects_empty_and_duplicate_prefs() {
        let c = config_with(|c| c.lang_preferences = vec![]);
        assert!(validate(&c).is_err());
        let c = config_with(|c| c.lang_preferences = vec!["en".into(), "EN".into()]);
        assert!(validate(&c).is_err());
    }

    #[test]
    fn validate_rejects_source_equal_to_target() {
        assert!(validate(&config_with(|c| c.source_lang = "en".into())).is_err());
        assert!(validate(&config_with(|c| {
            c.source_lang = "iw".into();
            c.target_lang = "he".into();
        }))
        .is_err());
        assert!(validate(&config_with(|c| c.source_lang = "auto".into())).is_ok());
        assert!(validate(&config_with(|c| {
            c.source_lang = "nl".into();
            c.target_lang = "en".into();
        }))
        .is_ok());
    }

    #[test]
    fn stored_config_without_source_lang_defaults_to_auto() {
        // Configs persisted before the field existed must keep loading.
        let c: Config = serde_json::from_value(json!({ "target_lang": "he" })).unwrap();
        assert_eq!(c.source_lang, "auto");
    }

    #[test]
    fn validate_rejects_bad_accent() {
        assert!(validate(&config_with(|c| c.accent = "blue".into())).is_err());
        assert!(validate(&config_with(|c| c.accent = "#12345".into())).is_err());
        assert!(validate(&config_with(|c| c.accent = "#4F46E5".into())).is_ok());
    }

    #[test]
    fn provider_kind_works_as_json_map_key() {
        let mut keys = HashMap::new();
        keys.insert(ProviderKind::DeepL, "abc".to_string());
        let round: HashMap<ProviderKind, String> =
            serde_json::from_str(&serde_json::to_string(&keys).unwrap()).unwrap();
        assert_eq!(round.get(&ProviderKind::DeepL).unwrap(), "abc");
    }
}
