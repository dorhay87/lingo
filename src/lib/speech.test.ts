import { describe, expect, it } from "vitest";
import { canSpeak, MAX_SPEAKABLE_CHARS } from "./speech";

describe("canSpeak", () => {
  it("requires text and a concrete language", () => {
    expect(canSpeak("hallo", "nl")).toBe(true);
    expect(canSpeak("", "nl")).toBe(false);
    expect(canSpeak("   ", "nl")).toBe(false);
    expect(canSpeak("hallo", null)).toBe(false);
    expect(canSpeak("hallo", "auto")).toBe(false);
  });

  it("rejects text beyond the endpoint budget", () => {
    expect(canSpeak("a".repeat(MAX_SPEAKABLE_CHARS), "en")).toBe(true);
    expect(canSpeak("a".repeat(MAX_SPEAKABLE_CHARS + 1), "en")).toBe(false);
  });
});
