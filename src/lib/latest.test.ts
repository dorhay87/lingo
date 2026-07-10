import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createLatestGuard, debounce } from "./latest";

describe("createLatestGuard", () => {
  it("accepts events for the newest issued id", () => {
    const guard = createLatestGuard();
    guard.issued(1);
    expect(guard.isCurrent(1)).toBe(true);
  });

  it("drops events from superseded requests", () => {
    const guard = createLatestGuard();
    guard.issued(1);
    guard.issued(2);
    expect(guard.isCurrent(1)).toBe(false);
    expect(guard.isCurrent(2)).toBe(true);
  });

  it("accepts an event that lands before its issue is recorded", () => {
    // The pending/result event can beat the invoke() promise resolution.
    const guard = createLatestGuard();
    guard.issued(3);
    expect(guard.isCurrent(4)).toBe(true);
    guard.issued(4);
    expect(guard.isCurrent(4)).toBe(true);
  });

  it("never regresses when issue acknowledgements arrive out of order", () => {
    const guard = createLatestGuard();
    guard.issued(5);
    guard.issued(4);
    expect(guard.latest()).toBe(5);
    expect(guard.isCurrent(4)).toBe(false);
  });

  it("invalidate drops in-flight results, e.g. after clearing the input", () => {
    const guard = createLatestGuard();
    guard.issued(3);
    guard.invalidate();
    expect(guard.isCurrent(3)).toBe(false);
    guard.issued(4);
    expect(guard.isCurrent(4)).toBe(true);
  });
});

describe("debounce", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("fires once after the wait with the last arguments", () => {
    const fn = vi.fn();
    const d = debounce(fn, 400);
    d.call("a");
    d.call("b");
    vi.advanceTimersByTime(399);
    expect(fn).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(fn).toHaveBeenCalledExactlyOnceWith("b");
  });

  it("cancel prevents the pending call", () => {
    const fn = vi.fn();
    const d = debounce(fn, 400);
    d.call("a");
    d.cancel();
    vi.advanceTimersByTime(1000);
    expect(fn).not.toHaveBeenCalled();
  });

  it("flush runs immediately and drops the pending call", () => {
    const fn = vi.fn();
    const d = debounce(fn, 400);
    d.call("late");
    d.flush("now");
    expect(fn).toHaveBeenCalledExactlyOnceWith("now");
    vi.advanceTimersByTime(1000);
    expect(fn).toHaveBeenCalledTimes(1);
  });
});
