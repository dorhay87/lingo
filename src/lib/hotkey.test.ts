import { describe, expect, it } from "vitest";
import { comboFromEvent } from "./hotkey";

const ev = (code: string, mods: Partial<Record<"ctrl" | "alt" | "shift" | "meta", boolean>> = {}) => ({
  ctrlKey: !!mods.ctrl,
  altKey: !!mods.alt,
  shiftKey: !!mods.shift,
  metaKey: !!mods.meta,
  code,
});

describe("comboFromEvent", () => {
  it("maps letter keys with modifiers", () => {
    expect(comboFromEvent(ev("KeyT", { ctrl: true }))).toBe("Ctrl+T");
    expect(comboFromEvent(ev("KeyT", { ctrl: true, shift: true }))).toBe(
      "Ctrl+Shift+T",
    );
  });

  it("maps digits, F-keys, space and navigation keys", () => {
    expect(comboFromEvent(ev("Digit1", { alt: true }))).toBe("Alt+1");
    expect(comboFromEvent(ev("F9", { alt: true }))).toBe("Alt+F9");
    expect(comboFromEvent(ev("Space", { ctrl: true }))).toBe("Ctrl+Space");
    expect(comboFromEvent(ev("ArrowUp", { ctrl: true }))).toBe("Ctrl+Up");
    expect(comboFromEvent(ev("Home", { ctrl: true }))).toBe("Ctrl+Home");
  });

  it("returns null while only modifiers are held", () => {
    expect(comboFromEvent(ev("ControlLeft", { ctrl: true }))).toBeNull();
    expect(comboFromEvent(ev("ShiftLeft", { shift: true }))).toBeNull();
  });

  it("rejects unmodified keys", () => {
    expect(comboFromEvent(ev("KeyT"))).toBeNull();
    expect(comboFromEvent(ev("F9"))).toBeNull();
  });
});
