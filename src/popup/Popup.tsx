import {
  For,
  Match,
  Show,
  Switch,
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
  type TranslationErrorEvent,
  type TranslationResultEvent,
  type UnlistenFn,
} from "../lib/ipc";
import { srcOptions as srcOptionsFor, tgtOptions as tgtOptionsFor } from "../lib/langOptions";
import { langName } from "../lib/langs";
import { createLatestGuard, debounce } from "../lib/latest";
import { canSpeak, createPlayer } from "../lib/speech";

const DEBOUNCE_MS = 400;

type Status = "idle" | "loading" | "result" | "error";

export function Popup() {
  const [config, setConfig] = createSignal<Config | null>(null);
  const [src, setSrc] = createSignal("auto");
  const [tgt, setTgt] = createSignal("en");
  const [text, setText] = createSignal("");
  const [status, setStatus] = createSignal<Status>("idle");
  const [result, setResult] = createSignal<TranslationResultEvent | null>(null);
  const [error, setError] = createSignal<TranslationErrorEvent | null>(null);
  const [pinned, setPinned] = createSignal(false);
  const [openMenu, setOpenMenu] = createSignal<"src" | "tgt" | null>(null);
  // False while a fresh show is being prepared; flipping back to true after
  // the popup_ready handshake restarts the enter animation on every open.
  const [entered, setEntered] = createSignal(true);

  let sourceRef!: HTMLTextAreaElement;
  let cardRef!: HTMLDivElement;
  let contentRef!: HTMLDivElement;

  const guard = createLatestGuard();

  // The card animates height changes in CSS (150ms ease-out). The window
  // must never clip the animation: grow it immediately, shrink it only after
  // the card has finished collapsing. On popup open the size snaps instead.
  // Floor matches MIN_CARD_HEIGHT in popup.rs: a webview hidden since
  // startup can report zero height before its first paint.
  const measureCard = () => Math.min(Math.max(contentRef.offsetHeight, 82), 450);
  let lastHeight = 0;
  const applyHeight = (height: number, animate: boolean) => {
    if (height === lastHeight && animate) return;
    const growing = height > lastHeight;
    lastHeight = height;
    if (!animate) {
      cardRef.style.transition = "none";
      requestAnimationFrame(() => (cardRef.style.transition = ""));
    }
    cardRef.style.height = `${height}px`;
    if (growing || !animate) {
      void commands.resizePopup(height);
    } else {
      setTimeout(() => {
        if (lastHeight === height) void commands.resizePopup(height);
      }, 170);
    }
  };

  const issue = async (value: string) => {
    const trimmed = value.trim();
    if (!trimmed) {
      // Nothing to translate; make sure a still-in-flight result for the
      // previous text can't render against an empty source pane.
      guard.invalidate();
      setStatus("idle");
      setResult(null);
      setError(null);
      return;
    }
    const id = await commands.translate(trimmed, src(), tgt());
    guard.issued(id);
  };

  const debounced = debounce(issue, DEBOUNCE_MS);

  const applyAccent = (c: Config) =>
    document.documentElement.style.setProperty("--accent-base", c.accent);

  const focusSource = () => {
    sourceRef.focus();
    const end = sourceRef.value.length;
    sourceRef.setSelectionRange(end, end);
  };

  const autosizeSource = () => {
    sourceRef.style.height = "auto";
    sourceRef.style.height = `${sourceRef.scrollHeight}px`;
  };

  onMount(async () => {
    const initial = await commands.getConfig();
    setConfig(initial);
    setTgt(initial.target_lang);
    applyAccent(initial);

    const unlisteners: UnlistenFn[] = [
      await events.onPopupShow(async (e) => {
        debounced.cancel();
        setEntered(false);
        setSrc(e.src);
        setTgt(e.tgt);
        setResult(null);
        setError(null);
        setPinned(false);
        setOpenMenu(null);
        setText(e.seed_text ?? "");
        setStatus("idle");
        autosizeSource();
        applyHeight(measureCard(), false);
        // State and size are clean: ask Rust to reveal the window, then play
        // the enter animation on the first visible frame.
        await commands.popupReady();
        requestAnimationFrame(() => {
          // Re-measure too: a webview hidden since startup lays out lazily
          // and can report a stale height before its first paint.
          applyHeight(measureCard(), false);
          setEntered(true);
        });
        focusSource();
        if (e.seed_text) void issue(e.seed_text);
      }),
      await events.onTranslationPending((e) => {
        if (guard.isCurrent(e.request_id)) {
          guard.issued(e.request_id);
          setStatus("loading");
        }
      }),
      await events.onTranslationResult((e) => {
        if (!guard.isCurrent(e.translation.request_id)) return;
        guard.issued(e.translation.request_id);
        setResult(e);
        setError(null);
        setStatus("result");
      }),
      await events.onTranslationError((e) => {
        if (!guard.isCurrent(e.request_id)) return;
        guard.issued(e.request_id);
        setError(e);
        setStatus("error");
      }),
      await events.onLangsChanged((e) => {
        setSrc(e.src);
        setTgt(e.tgt);
        void issue(text());
      }),
      await events.onPinChanged((e) => setPinned(e.pinned)),
      await events.onConfigChanged((c) => {
        setConfig(c);
        applyAccent(c);
      }),
    ];
    onCleanup(() => unlisteners.forEach((u) => u()));

    const observer = new ResizeObserver(() => applyHeight(measureCard(), true));
    observer.observe(contentRef);
    onCleanup(() => observer.disconnect());
  });

  const onInput = (value: string) => {
    setText(value);
    autosizeSource();
    debounced.call(value);
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      if (openMenu()) {
        setOpenMenu(null);
      } else {
        void commands.hidePopup();
      }
    }
  };

  const closeMenuOnOutsideClick = (e: MouseEvent) => {
    if (openMenu() && !(e.target as HTMLElement).closest(".menu-anchor")) {
      setOpenMenu(null);
    }
  };

  // An open language menu overflows the card; grow the window to cover it
  // (card height untouched) and restore the fit when it closes.
  createEffect(() => {
    if (!openMenu()) {
      if (lastHeight > 0) void commands.resizePopup(lastHeight);
      return;
    }
    requestAnimationFrame(() => {
      const menu = cardRef.querySelector(".menu");
      if (!menu) return;
      // Room below the menu for its rounded corners and shadow to render.
      const bottom =
        menu.getBoundingClientRect().bottom -
        cardRef.getBoundingClientRect().top +
        24;
      void commands.resizePopup(Math.max(lastHeight, bottom));
    });
  });

  const onSourceKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      // Only copy when a result is actually on screen; otherwise Enter with
      // no visible translation would copy a stale one.
      if (status() === "result") void commands.copyResult();
    }
  };

  const chooseLang = (which: "src" | "tgt", code: string) => {
    setOpenMenu(null);
    if (which === "src") setSrc(code);
    else setTgt(code);
    void commands.setLangs(src(), tgt());
    void issue(text());
  };

  const detected = createMemo(() => {
    const d = result()?.translation.detected_lang;
    return src() === "auto" && d ? d : null;
  });

  const canSwap = createMemo(() => src() !== "auto" || detected() !== null);

  const [speaking, setSpeaking] = createSignal<"source" | "result" | null>(null);
  const player = createPlayer();
  onCleanup(() => player.stop());

  const sourceSpeechLang = () => (src() !== "auto" ? src() : detected());

  const speakPane = async (pane: "source" | "result") => {
    if (speaking() === pane) {
      player.stop();
      setSpeaking(null);
      return;
    }
    const value =
      pane === "source" ? text().trim() : (result()?.translation.primary ?? "");
    const lang = pane === "source" ? sourceSpeechLang() : tgt();
    if (!canSpeak(value, lang)) return;
    setSpeaking(pane);
    try {
      const mp3 = await commands.speak(value, lang!);
      player.play(mp3, () => setSpeaking(null));
    } catch {
      setSpeaking(null);
    }
  };

  // Each menu omits the language selected on the other side, so from and to
  // can never end up equal.
  const srcOptions = createMemo(() =>
    srcOptionsFor(config()?.lang_preferences ?? [], tgt()),
  );
  const tgtOptions = createMemo(() =>
    tgtOptionsFor(config()?.lang_preferences ?? [], src()),
  );

  /** Dictionary display groups: alternatives joined with a same-pos
   * definition, then definition-only groups (e.g. when bd was absent). */
  const dictGroups = createMemo(() => {
    const t = result()?.translation;
    if (!t) return [];
    const groups = t.alternatives.map((entry) => ({
      pos: entry.pos,
      terms: entry.terms,
      definition:
        t.definitions.find((d) => d.pos === entry.pos)?.text ?? null,
    }));
    for (const def of t.definitions) {
      if (!groups.some((g) => g.pos === def.pos)) {
        groups.push({ pos: def.pos, terms: [], definition: def.text });
      }
    }
    return groups;
  });

  const hasDict = createMemo(() => dictGroups().length > 0);

  const footerHint = createMemo(() => {
    if (pinned()) return null; // rendered separately with the dot
    if (hasDict()) return "Tap an alternative to copy it";
    if (!result()) return "";
    return `${text().trim().length} characters`;
  });

  return (
    <div class="halo" onKeyDown={onKeyDown} onMouseDown={closeMenuOnOutsideClick}>
      <div
        class="card"
        classList={{ enter: entered(), "menu-open": openMenu() !== null }}
        ref={cardRef}
      >
        <div class="card-content" ref={contentRef}>
        <div class="header">
          <span class="menu-anchor">
            <button
              class="chip"
              onClick={() => setOpenMenu(openMenu() === "src" ? null : "src")}
            >
              {langName(src())}
              <Show when={detected()}>
                {(d) => <span class="detected">· {langName(d())}</span>}
              </Show>
              <ChevronIcon />
            </button>
            <Show when={openMenu() === "src"}>
              <div class="menu">
                <For each={srcOptions()}>
                  {(code) => (
                    <button
                      class="menu-item"
                      classList={{ selected: code === src() }}
                      onClick={() => chooseLang("src", code)}
                    >
                      {langName(code)}
                    </button>
                  )}
                </For>
              </div>
            </Show>
          </span>
          <button
            class="icon-btn"
            disabled={!canSwap()}
            title="Swap languages"
            onClick={() => void commands.swapLangs()}
          >
            <SwapIcon />
          </button>
          <span class="menu-anchor">
            <button
              class="chip"
              onClick={() => setOpenMenu(openMenu() === "tgt" ? null : "tgt")}
            >
              {langName(tgt())}
              <ChevronIcon />
            </button>
            <Show when={openMenu() === "tgt"}>
              <div class="menu">
                <For each={tgtOptions()}>
                  {(code) => (
                    <button
                      class="menu-item"
                      classList={{ selected: code === tgt() }}
                      onClick={() => chooseLang("tgt", code)}
                    >
                      {langName(code)}
                    </button>
                  )}
                </For>
              </div>
            </Show>
          </span>
          <span class="spacer" />
          <button
            class="icon-btn"
            title="Settings"
            onClick={() => void commands.openSettings()}
          >
            <CogIcon />
          </button>
          <button
            class="icon-btn"
            classList={{ active: pinned() }}
            title={pinned() ? "Unpin" : "Pin"}
            onClick={() => void commands.pinToggle()}
          >
            <PinIcon filled={pinned()} />
          </button>
        </div>

        <div class="divider" />
        <div class="source-row">
          <textarea
            ref={sourceRef}
            class="source"
            dir="auto"
            rows="1"
            spellcheck={false}
            placeholder="Translate…"
            value={text()}
            onInput={(e) => onInput(e.currentTarget.value)}
            onKeyDown={onSourceKeyDown}
          />
          <Show when={canSpeak(text(), sourceSpeechLang())}>
            <button
              class="speak-btn"
              classList={{ active: speaking() === "source" }}
              title="Listen"
              onClick={() => void speakPane("source")}
            >
              <SpeakerIcon />
            </button>
          </Show>
        </div>

        <Show when={status() !== "idle"}>
          <div class="divider" />
          <Switch>
            <Match when={status() === "loading"}>
              <div class="loading">
                <span />
                <span />
                <span />
              </div>
            </Match>

            <Match when={status() === "error"}>
              <div class="error-row">
                <WarnIcon />
                <span class="message">{error()?.message}</span>
                <span class="spacer" />
                <Show when={error()?.retryable}>
                  <button class="text-btn" onClick={() => void issue(text())}>
                    <RetryIcon />
                    Retry
                  </button>
                </Show>
              </div>
            </Match>

            <Match when={status() === "result" && result()}>
              {(r) => (
                <>
                  <div class="result-row">
                    <div
                      class="result"
                      classList={{
                        "with-dict": hasDict(),
                        long: r().translation.primary.length > 240,
                      }}
                      dir="auto"
                    >
                      {r().translation.primary}
                    </div>
                    <Show when={canSpeak(r().translation.primary, tgt())}>
                      <button
                        class="speak-btn"
                        classList={{ active: speaking() === "result" }}
                        title="Listen"
                        onClick={() => void speakPane("result")}
                      >
                        <SpeakerIcon />
                      </button>
                    </Show>
                  </div>
                  <Show when={hasDict()}>
                    <div class="dict">
                      <For each={dictGroups()}>
                        {(group) => (
                          <>
                            <div class="pos">{group.pos}</div>
                            <Show when={group.terms.length > 0}>
                              <div class="terms" dir="auto">
                                <For each={group.terms}>
                                  {(term, i) => (
                                    <>
                                      <Show when={i() > 0}>
                                        <span class="dot">·</span>
                                      </Show>
                                      <button
                                        class={`term rank-${Math.min(i(), 3)}`}
                                        dir="auto"
                                        onClick={() =>
                                          void commands.copyText(term.term)
                                        }
                                      >
                                        {term.term}
                                      </button>
                                    </>
                                  )}
                                </For>
                              </div>
                            </Show>
                            <Show when={group.definition}>
                              <div class="definition" dir="auto">
                                {group.definition}
                              </div>
                            </Show>
                          </>
                        )}
                      </For>
                    </div>
                  </Show>
                  <div class="footer">
                    <Show
                      when={pinned()}
                      fallback={<span class="hint">{footerHint()}</span>}
                    >
                      <span class="hint pinned">Pinned</span>
                    </Show>
                    <button
                      class="text-btn"
                      onClick={() => void commands.copyResult()}
                    >
                      <CopyIcon />
                      Copy <kbd>Enter</kbd>
                    </button>
                  </div>
                </>
              )}
            </Match>
          </Switch>
        </Show>
        </div>
      </div>
    </div>
  );
}

function ChevronIcon() {
  return (
    <svg width="9" height="9" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
      <path d="M6 9l6 6 6-6" />
    </svg>
  );
}

function SwapIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
      <path d="M4 8h13l-3-3M20 16H7l3 3" />
    </svg>
  );
}

function PinIcon(props: { filled: boolean }) {
  return (
    <svg
      width="15"
      height="15"
      viewBox="0 0 24 24"
      fill={props.filled ? "currentColor" : "none"}
      stroke="currentColor"
      stroke-width={props.filled ? "1.4" : "1.6"}
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <path d="M9 4h6l-1 5 3 3v2H7v-2l3-3-1-5z" />
      <line x1="12" y1="17" x2="12" y2="21" fill="none" />
    </svg>
  );
}

function SpeakerIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
      <path d="M11 5 6 9H2v6h4l5 4V5z" />
      <path d="M15.5 8.5a5 5 0 0 1 0 7" />
    </svg>
  );
}

function CogIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h.01a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51h.01a1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82v.01a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
    </svg>
  );
}

function WarnIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
      <path d="M12 9v4M12 17h.01" />
      <path d="M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z" />
    </svg>
  );
}

function RetryIcon() {
  return (
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
      <path d="M3 12a9 9 0 1 0 3-6.7L3 8" />
      <path d="M3 3v5h5" />
    </svg>
  );
}

function CopyIcon() {
  return (
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
      <rect x="9" y="9" width="11" height="11" rx="2" />
      <path d="M5 15V5a2 2 0 0 1 2-2h10" />
    </svg>
  );
}
