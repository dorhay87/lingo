import { describe, expect, it } from "vitest";
import { srcOptions, tgtOptions } from "./langOptions";

const PREFS = ["en", "he", "nl"];

describe("language menu options", () => {
  it("source menu offers auto plus preferences without the target", () => {
    expect(srcOptions(PREFS, "en")).toEqual(["auto", "he", "nl"]);
    expect(srcOptions(PREFS, "nl")).toEqual(["auto", "en", "he"]);
  });

  it("target menu offers preferences without the explicit source", () => {
    expect(tgtOptions(PREFS, "he")).toEqual(["en", "nl"]);
  });

  it("auto source does not remove anything from the target menu", () => {
    expect(tgtOptions(PREFS, "auto")).toEqual(PREFS);
  });

  it("from and to can never be equal via the menus", () => {
    for (const tgt of PREFS) {
      expect(srcOptions(PREFS, tgt)).not.toContain(tgt);
    }
    for (const src of PREFS) {
      expect(tgtOptions(PREFS, src)).not.toContain(src);
    }
  });

  it("single-language list leaves only auto as a source choice", () => {
    expect(srcOptions(["en"], "en")).toEqual(["auto"]);
    expect(tgtOptions(["en"], "auto")).toEqual(["en"]);
  });
});
