// Single place where TS mirrors the Rust types and wraps Tauri IPC.
// Frontend stays a dumb renderer: it invokes intents and listens to broadcasts.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Lang = string;

export type ProviderKind = "GoogleFree" | "DeepL";

export type Theme = "System" | "Light" | "Dark";

export interface Config {
  hotkey: string;
  provider: ProviderKind;
  api_keys: Partial<Record<ProviderKind, string>>;
  source_lang: Lang;
  target_lang: Lang;
  lang_preferences: Lang[];
  launch_at_startup: boolean;
  theme: Theme;
  accent: string;
}

export interface ScoredTerm {
  term: string;
  score: number;
}

export interface DictEntry {
  pos: string;
  terms: ScoredTerm[];
}

export interface Definition {
  pos: string;
  text: string;
}

export interface Translation {
  request_id: number;
  primary: string;
  alternatives: DictEntry[];
  definitions: Definition[];
  detected_lang: Lang | null;
}

export interface TranslationResultEvent {
  translation: Translation;
  provider: ProviderKind;
}

export interface TranslationErrorEvent {
  request_id: number;
  message: string;
  retryable: boolean;
}

export interface PopupShowEvent {
  seed_text: string | null;
  src: Lang;
  tgt: Lang;
}

export interface PinChangedEvent {
  pinned: boolean;
}

// ---- Commands (frontend intents) ----

export const commands = {
  translate: (text: string, src: Lang, tgt: Lang) =>
    invoke<number>("translate", { text, src, tgt }),
  setLangs: (src: Lang, tgt: Lang) => invoke<void>("set_langs", { src, tgt }),
  swapLangs: () => invoke<void>("swap_langs"),
  updateConfig: (patch: Partial<Config>) =>
    invoke<void>("update_config", { patch }),
  getConfig: () => invoke<Config>("get_config"),
  copyResult: () => invoke<void>("copy_result"),
  copyText: (text: string) => invoke<void>("copy_text", { text }),
  pinToggle: () => invoke<void>("pin_toggle"),
  hidePopup: () => invoke<void>("hide_popup"),
  popupReady: () => invoke<void>("popup_ready"),
  openSettings: () => invoke<void>("open_settings"),
  resizePopup: (height: number) => invoke<void>("resize_popup", { height }),
  testProvider: (provider: ProviderKind, apiKey: string) =>
    invoke<void>("test_provider", { provider, apiKey }),
  speak: (text: string, lang: Lang) =>
    invoke<string>("speak", { text, lang }),
};

/** Error shape thrown by update_config when a hotkey is already taken. */
export interface ConfigError {
  kind: "hotkey_in_use" | "invalid_hotkey" | "invalid_config" | "io";
  message: string;
}

export interface ProviderTestError {
  kind: "network" | "rate_limited" | "auth_failed";
  message: string;
}

// ---- Events (Rust broadcasts) ----

export const events = {
  onTranslationPending: (cb: (e: { request_id: number }) => void) =>
    listen<{ request_id: number }>("translation:pending", (ev) => cb(ev.payload)),
  onTranslationResult: (cb: (e: TranslationResultEvent) => void) =>
    listen<TranslationResultEvent>("translation:result", (ev) => cb(ev.payload)),
  onTranslationError: (cb: (e: TranslationErrorEvent) => void) =>
    listen<TranslationErrorEvent>("translation:error", (ev) => cb(ev.payload)),
  onConfigChanged: (cb: (c: Config) => void) =>
    listen<Config>("config:changed", (ev) => cb(ev.payload)),
  onPopupShow: (cb: (e: PopupShowEvent) => void) =>
    listen<PopupShowEvent>("popup:show", (ev) => cb(ev.payload)),
  onPinChanged: (cb: (e: PinChangedEvent) => void) =>
    listen<PinChangedEvent>("popup:pin_changed", (ev) => cb(ev.payload)),
  onLangsChanged: (cb: (e: { src: Lang; tgt: Lang }) => void) =>
    listen<{ src: Lang; tgt: Lang }>("popup:langs", (ev) => cb(ev.payload)),
};

export type { UnlistenFn };
