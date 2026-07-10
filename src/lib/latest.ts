// Pure helpers for the live-translate flow: input debouncing and dropping
// events that belong to superseded requests.

/**
 * Tracks the newest issued request_id. Rust aborts stale in-flight provider
 * calls; this guard covers the frontend half of the race, where an older
 * result event can still arrive after a newer request was issued.
 */
export function createLatestGuard() {
  let latest = 0;
  return {
    /** Record an issued id (ids are monotonic but responses may interleave). */
    issued(id: number) {
      latest = Math.max(latest, id);
    },
    /** Should an event carrying this id still be rendered? */
    isCurrent(id: number) {
      return id >= latest;
    },
    /** Drop everything issued so far, e.g. when the input was cleared and no
     * in-flight result should render anymore. */
    invalidate() {
      latest += 1;
    },
    latest: () => latest,
  };
}

export function debounce<A extends unknown[]>(
  fn: (...args: A) => void,
  ms: number,
): { call: (...args: A) => void; cancel: () => void; flush: (...args: A) => void } {
  let timer: ReturnType<typeof setTimeout> | undefined;
  return {
    call(...args: A) {
      clearTimeout(timer);
      timer = setTimeout(() => fn(...args), ms);
    },
    cancel() {
      clearTimeout(timer);
    },
    /** Run immediately, cancelling any pending call. */
    flush(...args: A) {
      clearTimeout(timer);
      fn(...args);
    },
  };
}
