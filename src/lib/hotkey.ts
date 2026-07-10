/** Map a keydown to a hotkey string Rust can register ("Ctrl+Shift+T"), or
 * null while the combination is incomplete. Requires at least one modifier
 * plus a non-modifier key. */
export function comboFromEvent(e: {
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
  code: string;
}): string | null {
  const mods: string[] = [];
  if (e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push("Super");

  let key: string | null = null;
  const code = e.code;
  if (code.startsWith("Key")) key = code.slice(3);
  else if (code.startsWith("Digit")) key = code.slice(5);
  else if (/^F\d{1,2}$/.test(code)) key = code;
  else if (code === "Space") key = "Space";
  else if (
    ["Home", "End", "PageUp", "PageDown", "Insert", "Delete"].includes(code)
  )
    key = code;
  else if (code.startsWith("Arrow")) key = code.slice(5);

  if (!key || mods.length === 0) return null;
  return [...mods, key].join("+");
}
