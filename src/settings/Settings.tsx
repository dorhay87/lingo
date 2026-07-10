import {
  For,
  Show,
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
} from "solid-js";
import {
  commands,
  events,
  type Config,
  type ConfigError,
  type ProviderKind,
  type ProviderTestError,
  type Theme,
  type UnlistenFn,
} from "../lib/ipc";
import { comboFromEvent } from "../lib/hotkey";
import { LANG_LIST, langName } from "../lib/langs";

const ACCENT_PRESETS = ["#4F46E5", "#0F9D8C", "#2E8B57", "#C2410C", "#9333EA"];

const PROVIDERS: { kind: ProviderKind; title: string; subtitle: string }[] = [
  { kind: "GoogleFree", title: "Google", subtitle: "No key needed" },
  { kind: "DeepL", title: "DeepL", subtitle: "API key" },
];

const THEMES: Theme[] = ["System", "Light", "Dark"];

export function Settings() {
  const [config, setConfig] = createSignal<Config | null>(null);

  const applyAccent = (c: Config) =>
    document.documentElement.style.setProperty("--accent-base", c.accent);

  onMount(async () => {
    const initial = await commands.getConfig();
    setConfig(initial);
    applyAccent(initial);
    const unlisten: UnlistenFn = await events.onConfigChanged((c) => {
      setConfig(c);
      applyAccent(c);
    });
    onCleanup(unlisten);
  });

  const patch = (p: Partial<Config>) =>
    commands.updateConfig(p).catch((e: ConfigError) => {
      console.error("config update rejected:", e);
    });

  return (
    <Show when={config()}>
      {(cfg) => (
        <main class="settings">
          <HotkeySection config={cfg()} />
          <LanguagesSection config={cfg()} patch={patch} />
          <ProviderSection config={cfg()} patch={patch} />
          <GeneralSection config={cfg()} patch={patch} />
        </main>
      )}
    </Show>
  );
}

// ---- Hotkey ----

function HotkeySection(props: { config: Config }) {
  const [recording, setRecording] = createSignal(false);
  const [conflict, setConflict] = createSignal<string | null>(null);

  const startRecording = () => {
    setConflict(null);
    setRecording(true);
    const onKey = async (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") {
        stop();
        return;
      }
      const combo = comboFromEvent(e);
      if (!combo) return; // modifiers only so far
      stop();
      try {
        await commands.updateConfig({ hotkey: combo });
      } catch (err) {
        const ce = err as ConfigError;
        setConflict(
          ce.kind === "hotkey_in_use"
            ? "This shortcut is used by another app."
            : ce.message,
        );
      }
    };
    const stop = () => {
      window.removeEventListener("keydown", onKey, true);
      setRecording(false);
    };
    window.addEventListener("keydown", onKey, true);
  };

  return (
    <section>
      <div class="section-title">Hotkey</div>
      <div class="panel">
        <Show
          when={!recording()}
          fallback={
            <div class="hotkey-recording">
              <div class="prompt">Press a combination…</div>
              <div class="cancel-hint">Esc to cancel</div>
            </div>
          }
        >
          <div class="row">
            <span class="row-label">Global activation shortcut</span>
            <button class="hotkey-combo" onClick={startRecording}>
              <For each={props.config.hotkey.split("+")}>
                {(part, i) => (
                  <>
                    <Show when={i() > 0}>
                      <span class="plus">+</span>
                    </Show>
                    <kbd>{part}</kbd>
                  </>
                )}
              </For>
            </button>
          </div>
        </Show>
      </div>
      <Show when={conflict()}>
        <div class="warn-banner">
          <WarnIcon />
          <span>{conflict()}</span>
        </div>
      </Show>
    </section>
  );
}

// ---- Languages ----

function LanguagesSection(props: {
  config: Config;
  patch: (p: Partial<Config>) => Promise<void>;
}) {
  const [pickerOpen, setPickerOpen] = createSignal(false);
  const [query, setQuery] = createSignal("");
  const [dragIndex, setDragIndex] = createSignal<number | null>(null);
  const [dragOver, setDragOver] = createSignal<number | null>(null);

  const prefs = () => props.config.lang_preferences;

  const filtered = createMemo(() => {
    const q = query().trim().toLowerCase();
    return LANG_LIST.filter(
      (l) =>
        !q ||
        l.name.toLowerCase().includes(q) ||
        l.code.toLowerCase().startsWith(q),
    ).slice(0, 30);
  });

  const addLang = (code: string) => {
    setPickerOpen(false);
    setQuery("");
    void props.patch({ lang_preferences: [...prefs(), code] });
  };

  const removeLang = (code: string) =>
    void props.patch({
      lang_preferences: prefs().filter((c) => c !== code),
    });

  const drop = (target: number) => {
    const from = dragIndex();
    setDragIndex(null);
    setDragOver(null);
    if (from === null || from === target) return;
    const next = [...prefs()];
    const [moved] = next.splice(from, 1);
    next.splice(target, 0, moved);
    void props.patch({ lang_preferences: next });
  };

  return (
    <section>
      <div class="section-title">Languages</div>
      <div class="panel">
        <div class="row">
          <span class="row-label">Default target language</span>
          <select
            class="select"
            value={props.config.target_lang}
            onChange={(e) =>
              void props.patch({ target_lang: e.currentTarget.value })
            }
          >
            <For each={prefs()}>
              {(code) => <option value={code}>{langName(code)}</option>}
            </For>
          </select>
        </div>
        <div class="divider" />
        <div class="note">
          Languages offered in the popup dropdowns, in this order
        </div>
        <For each={prefs()}>
          {(code, i) => (
            <div
              class="lang-row"
              classList={{ "drag-over": dragOver() === i() }}
              draggable="true"
              onDragStart={() => setDragIndex(i())}
              onDragOver={(e) => {
                e.preventDefault();
                setDragOver(i());
              }}
              onDragLeave={() => setDragOver(null)}
              onDrop={() => drop(i())}
              onDragEnd={() => {
                setDragIndex(null);
                setDragOver(null);
              }}
            >
              <span class="grip">
                <GripIcon />
              </span>
              <span class="name">{langName(code)}</span>
              <button
                class="remove"
                title="Remove"
                disabled={prefs().length <= 1}
                onClick={() => removeLang(code)}
              >
                <XIcon />
              </button>
            </div>
          )}
        </For>
        <div class="divider" />
        <Show
          when={pickerOpen()}
          fallback={
            <button class="add-lang" onClick={() => setPickerOpen(true)}>
              <PlusIcon />
              Add language…
            </button>
          }
        >
          <div class="lang-picker">
            <div class="search">
              <SearchIcon />
              <input
                autofocus
                placeholder="Search languages"
                value={query()}
                onInput={(e) => setQuery(e.currentTarget.value)}
                onKeyDown={(e) => {
                  if (e.key === "Escape") {
                    setPickerOpen(false);
                    setQuery("");
                  }
                }}
              />
            </div>
            <div class="options">
              <For each={filtered()}>
                {(lang) => {
                  const added = () => prefs().includes(lang.code);
                  return (
                    <button
                      class="option"
                      disabled={added()}
                      onClick={() => addLang(lang.code)}
                    >
                      {lang.name}
                      <Show when={added()}>
                        <span class="tag">already added</span>
                      </Show>
                    </button>
                  );
                }}
              </For>
            </div>
          </div>
        </Show>
      </div>
    </section>
  );
}

// ---- Provider ----

function ProviderSection(props: {
  config: Config;
  patch: (p: Partial<Config>) => Promise<void>;
}) {
  const [reveal, setReveal] = createSignal(false);
  const [testing, setTesting] = createSignal(false);
  const [testResult, setTestResult] = createSignal<
    { ok: boolean; message: string } | null
  >(null);

  const selected = () => props.config.provider;
  const needsKey = () => selected() !== "GoogleFree";

  // Local draft of the key field: Test must exercise what the user typed,
  // not whatever the config held before the blur-save round-tripped.
  const [keyDraft, setKeyDraft] = createSignal("");
  createEffect(() => setKeyDraft(props.config.api_keys[selected()] ?? ""));

  const choose = (kind: ProviderKind) => {
    setTestResult(null);
    setReveal(false);
    void props.patch({ provider: kind });
  };

  const saveKey = (value: string) =>
    void props.patch({
      api_keys: { ...props.config.api_keys, [selected()]: value },
    });

  const runTest = async () => {
    setTesting(true);
    setTestResult(null);
    saveKey(keyDraft());
    try {
      await commands.testProvider(selected(), keyDraft());
      setTestResult({ ok: true, message: "Connected - key valid" });
    } catch (err) {
      const pe = err as ProviderTestError;
      setTestResult({ ok: false, message: pe.message });
    } finally {
      setTesting(false);
    }
  };

  return (
    <section>
      <div class="section-title">Provider</div>
      <div class="provider-cards">
        <For each={PROVIDERS}>
          {(p) => (
            <button
              class="provider-card"
              classList={{ selected: selected() === p.kind }}
              onClick={() => choose(p.kind)}
            >
              <span class="radio" />
              <span>
                <div class="title">{p.title}</div>
                <div class="subtitle">{p.subtitle}</div>
              </span>
            </button>
          )}
        </For>
      </div>
      <Show when={needsKey()}>
        <div class="api-key-block">
          <div class="label">API key</div>
          <div class="api-key-row">
            <div class="api-key-input">
              <input
                type={reveal() ? "text" : "password"}
                value={keyDraft()}
                onInput={(e) => setKeyDraft(e.currentTarget.value)}
                onChange={(e) => saveKey(e.currentTarget.value)}
                placeholder="Paste your key"
              />
              <button
                class="eye"
                title={reveal() ? "Hide key" : "Show key"}
                onClick={() => setReveal(!reveal())}
              >
                <EyeIcon />
              </button>
            </div>
            <button
              class="btn-primary"
              disabled={testing()}
              onClick={() => void runTest()}
            >
              {testing() ? "Testing…" : "Test"}
            </button>
          </div>
          <Show when={testResult()}>
            {(r) => (
              <div class="test-status" classList={{ ok: r().ok, fail: !r().ok }}>
                <Show when={r().ok} fallback={<WarnIcon />}>
                  <CheckIcon />
                </Show>
                {r().message}
              </div>
            )}
          </Show>
        </div>
      </Show>
    </section>
  );
}

// ---- General ----

function GeneralSection(props: {
  config: Config;
  patch: (p: Partial<Config>) => Promise<void>;
}) {
  return (
    <section>
      <div class="section-title">General</div>
      <div class="panel">
        <div class="row">
          <span class="row-label">Launch at startup</span>
          <button
            class="switch"
            classList={{ on: props.config.launch_at_startup }}
            role="switch"
            aria-checked={props.config.launch_at_startup}
            onClick={() =>
              void props.patch({
                launch_at_startup: !props.config.launch_at_startup,
              })
            }
          />
        </div>
        <div class="divider" />
        <div class="row">
          <span class="row-label">Theme</span>
          <span class="segmented">
            <For each={THEMES}>
              {(t) => (
                <button
                  classList={{ active: props.config.theme === t }}
                  onClick={() => void props.patch({ theme: t })}
                >
                  {t}
                </button>
              )}
            </For>
          </span>
        </div>
        <div class="divider" />
        <div class="row">
          <span class="row-label">Accent color</span>
          <span class="accent-swatches">
            <For each={ACCENT_PRESETS}>
              {(color) => (
                <button
                  class="swatch"
                  classList={{
                    selected:
                      props.config.accent.toLowerCase() ===
                      color.toLowerCase(),
                  }}
                  style={{ background: color, "--swatch-color": color }}
                  title={color}
                  onClick={() => void props.patch({ accent: color })}
                />
              )}
            </For>
            <span class="sep" />
            <span class="custom" title="Custom color">
              <input
                type="color"
                value={props.config.accent}
                onChange={(e) =>
                  void props.patch({ accent: e.currentTarget.value })
                }
              />
            </span>
          </span>
        </div>
      </div>
    </section>
  );
}

// ---- icons ----

function WarnIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
      <path d="M12 9v4M12 17h.01" />
      <path d="M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z" />
    </svg>
  );
}

function CheckIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
      <path d="M20 6 9 17l-5-5" />
    </svg>
  );
}

function GripIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor">
      <circle cx="9" cy="6" r="1.6" />
      <circle cx="15" cy="6" r="1.6" />
      <circle cx="9" cy="12" r="1.6" />
      <circle cx="15" cy="12" r="1.6" />
      <circle cx="9" cy="18" r="1.6" />
      <circle cx="15" cy="18" r="1.6" />
    </svg>
  );
}

function XIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.6" fill="none" stroke-linecap="round">
      <line x1="6" y1="6" x2="18" y2="18" />
      <line x1="18" y1="6" x2="6" y2="18" />
    </svg>
  );
}

function PlusIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.7" fill="none" stroke-linecap="round">
      <line x1="12" y1="5" x2="12" y2="19" />
      <line x1="5" y1="12" x2="19" y2="12" />
    </svg>
  );
}

function SearchIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round">
      <circle cx="11" cy="11" r="7" />
      <line x1="21" y1="21" x2="16.5" y2="16.5" />
    </svg>
  );
}

function EyeIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
      <path d="M1 12s4-7 11-7 11 7 11 7-4 7-11 7-11-7-11-7z" />
      <circle cx="12" cy="12" r="3" />
    </svg>
  );
}
