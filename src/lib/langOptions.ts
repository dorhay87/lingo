import type { Lang } from "./ipc";

/** Source menu: auto plus every preference except the current target, so
 * from and to can never be set to the same language. */
export function srcOptions(preferences: Lang[], tgt: Lang): Lang[] {
  return ["auto", ...preferences.filter((c) => c !== tgt)];
}

/** Target menu: every preference except the current explicit source. */
export function tgtOptions(preferences: Lang[], src: Lang): Lang[] {
  return preferences.filter((c) => c !== src);
}
