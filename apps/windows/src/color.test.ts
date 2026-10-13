// The colour arithmetic the themes lean on.
import { describe, expect, it } from "vitest";
import { contrast, isDark, luminance, mix, parseHex, rgba, scale, toHex } from "./color";

describe("hex", () => {
  it("reads a colour with or without its hash, in either case", () => {
    expect(parseHex("#1e1e2e")).toEqual([30, 30, 46]);
    expect(parseHex("A6E3A1")).toEqual([166, 227, 161]);
    expect(parseHex("  #ffffff ")).toEqual([255, 255, 255]);
  });
  it("takes nothing else for a colour", () => {
    expect(parseHex("#fff")).toBeUndefined();
    expect(parseHex("#12345g")).toBeUndefined();
    expect(parseHex("white")).toBeUndefined();
    expect(parseHex("")).toBeUndefined();
  });
  it("writes it back in lower case, holding the channels to a byte", () => {
    expect(toHex([30, 30, 46])).toBe("#1e1e2e");
    expect(toHex([300, -4, 15.6])).toBe("#ff0010");
  });
});

describe("light", () => {
  it("is the macOS app's: the channels as encoded, weighted, halfway is the line", () => {
    expect(luminance([255, 255, 255])).toBeCloseTo(1, 10);
    expect(luminance([0, 0, 0])).toBe(0);
    expect(isDark([40, 44, 52])).toBe(true);
    expect(isDark([255, 255, 255])).toBe(false);
    // 0.2126 + 0.7152 of full light is already past half: a bright green is light.
    expect(isDark([0, 255, 0])).toBe(false);
    expect(isDark([0, 0, 255])).toBe(true);
    expect(isDark([127, 127, 127])).toBe(true);
    expect(isDark([128, 128, 128])).toBe(false);
  });
});

describe("the steps", () => {
  it("scales each channel, rounding, and holds it to a byte", () => {
    expect(scale([40, 44, 52], 0.72)).toEqual([29, 32, 37]);
    expect(scale([255, 255, 255], 0.955)).toEqual([244, 244, 244]);
    expect(scale([200, 200, 200], 1.5)).toEqual([255, 255, 255]);
  });
  it("mixes toward another colour", () => {
    expect(mix([0, 0, 0], [200, 100, 50], 0.5)).toEqual([100, 50, 25]);
    expect(mix([10, 20, 30], [90, 80, 70], 0)).toEqual([10, 20, 30]);
    expect(mix([10, 20, 30], [90, 80, 70], 1)).toEqual([90, 80, 70]);
  });
  it("writes a translucent ground as CSS", () => {
    expect(rgba([31, 34, 41], 0.86)).toBe("rgba(31, 34, 41, 0.86)");
  });
});

describe("contrast", () => {
  it("is WCAG's, 1 to 21", () => {
    expect(contrast([0, 0, 0], [255, 255, 255])).toBeCloseTo(21, 5);
    expect(contrast([255, 255, 255], [0, 0, 0])).toBeCloseTo(21, 5);
    expect(contrast([120, 120, 120], [120, 120, 120])).toBe(1);
    // The grey #777777 on white is the textbook 4.48.
    expect(contrast([119, 119, 119], [255, 255, 255])).toBeCloseTo(4.48, 2);
  });
});
