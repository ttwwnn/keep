// The translucent ground: what is asked for, what the system gives, the colours that come of it.
import { describe, expect, it } from "vitest";
import {
  acrylicDrags,
  blurChoices,
  defaultGlass,
  effectiveAlpha,
  effectiveBlur,
  glassTheme,
  glassVars,
  holdsOpaque,
  needsRestart,
  normalizeGlass,
  normalizeOpacity,
  translucent,
} from "./glass";

describe("the state", () => {
  it("is the opaque window until it is asked otherwise", () => {
    expect(defaultGlass()).toEqual({ opacity: 1, blur: "none" });
    expect(normalizeGlass(undefined)).toEqual(defaultGlass());
    expect(normalizeGlass(null)).toEqual(defaultGlass());
    expect(normalizeGlass([])).toEqual(defaultGlass());
    expect(normalizeGlass("mica")).toEqual(defaultGlass());
  });
  it("keeps a good choice whole", () => {
    expect(normalizeGlass({ opacity: 0.8, blur: "acrylic" })).toEqual({ opacity: 0.8, blur: "acrylic" });
    expect(normalizeGlass({ opacity: 0.5, blur: "mica" })).toEqual({ opacity: 0.5, blur: "mica" });
  });
  it("holds the opacity between half and whole, in steps of a per cent, and takes the rest for opaque", () => {
    expect(normalizeOpacity(0.2)).toBe(0.5);
    expect(normalizeOpacity(3)).toBe(1);
    expect(normalizeOpacity(0.834)).toBe(0.83);
    expect(normalizeOpacity("0.7")).toBe(1);
    expect(normalizeOpacity(NaN)).toBe(1);
    expect(normalizeOpacity(Infinity)).toBe(1);
  });
  it("takes an unknown material for none and loses nothing else", () => {
    expect(normalizeGlass({ opacity: 0.7, blur: "vibrancy" })).toEqual({ opacity: 0.7, blur: "none" });
    expect(normalizeGlass({ blur: "mica" })).toEqual({ opacity: 1, blur: "mica" });
  });
});

describe("what the system gives", () => {
  it("offers Mica from Windows 11 22H2 and acrylic from 1809", () => {
    expect(blurChoices("windows", 26100)).toEqual(["none", "mica", "acrylic"]);
    expect(blurChoices("windows", 22621)).toEqual(["none", "mica", "acrylic"]);
    expect(blurChoices("windows", 22000)).toEqual(["none", "acrylic"]);
    expect(blurChoices("windows", 19045)).toEqual(["none", "acrylic"]);
    expect(blurChoices("windows", 17763)).toEqual(["none", "acrylic"]);
    expect(blurChoices("windows", 17134)).toEqual(["none"]);
    expect(blurChoices("windows", 0)).toEqual(["none"]);
  });
  it("offers nothing where the translucency is not the app's", () => {
    expect(blurChoices("macos", 0)).toEqual(["none"]);
    expect(blurChoices("linux", 0)).toEqual(["none"]);
  });
  it("applies the material asked for only where it exists, and only to a ground that lets it through", () => {
    const mica = { opacity: 0.8, blur: "mica" } as const;
    expect(effectiveBlur(mica, "windows", 26100)).toBe("mica");
    expect(effectiveBlur(mica, "windows", 19045)).toBe("none");
    expect(effectiveBlur(mica, "macos", 0)).toBe("none");
    expect(effectiveBlur({ opacity: 1, blur: "mica" }, "windows", 26100)).toBe("none");
  });
  it("warns of acrylic only on the Windows that stutters with it", () => {
    expect(acrylicDrags("windows", 19045)).toBe(true);
    expect(acrylicDrags("windows", 17763)).toBe(true);
    expect(acrylicDrags("windows", 22000)).toBe(false);
    expect(acrylicDrags("windows", 26100)).toBe(false);
    expect(acrylicDrags("windows", 17134)).toBe(false);
    expect(acrylicDrags("macos", 0)).toBe(false);
  });
});

describe("how much is painted", () => {
  const glass = { opacity: 0.7, blur: "none" } as const;
  it("is the opacity asked for in a window that was made transparent", () => {
    expect(effectiveAlpha(glass, true, false)).toBe(0.7);
  });
  it("is whole in a window that was not, however much is asked", () => {
    expect(effectiveAlpha(glass, false, false)).toBe(1);
  });
  it("is whole while a picker is held over it", () => {
    expect(effectiveAlpha(glass, true, true)).toBe(1);
  });
  it("asks to open the app again only for a translucency the window cannot have", () => {
    expect(needsRestart(glass, false)).toBe(true);
    expect(needsRestart(glass, true)).toBe(false);
    expect(needsRestart(defaultGlass(), false)).toBe(false);
    expect(needsRestart(defaultGlass(), true)).toBe(false);
  });
  it("holds the window opaque for the pickers and for nothing else", () => {
    for (const overlay of ["picker", "palette", "themes", "fonts"]) expect(holdsOpaque(overlay)).toBe(true);
    for (const overlay of [null, "settings", "find", "search"]) expect(holdsOpaque(overlay)).toBe(false);
  });
});

describe("the colours", () => {
  it("writes a ground at an alpha, and leaves a whole one as it was", () => {
    expect(translucent("#282c34", 0.8)).toBe("rgba(40, 44, 52, 0.8)");
    expect(translucent("#282c34", 1)).toBe("#282c34");
  });
  it("leaves what is no hex colour alone", () => {
    expect(translucent("rgb(1, 2, 3)", 0.5)).toBe("rgb(1, 2, 3)");
    expect(translucent("", 0.5)).toBe("");
  });
  it("makes the terminal's background translucent and touches nothing else of the theme", () => {
    const theme = { background: "#ffffff", foreground: "#1f2328", red: "#cf222e", selectionBackground: "#b4d5ff" };
    expect(glassTheme(theme, 0.6)).toEqual({ ...theme, background: "rgba(255, 255, 255, 0.6)" });
    expect(theme.background).toBe("#ffffff");
  });
  it("hands the same theme back when nothing changes, so that nothing follows it for nothing", () => {
    const theme = { background: "#282c34", foreground: "#ffffff" };
    expect(glassTheme(theme, 1)).toBe(theme);
    expect(glassTheme({ foreground: "#fff" }, 0.5)).toEqual({ foreground: "#fff" });
    const odd = { background: "transparent" };
    expect(glassTheme(odd, 0.5)).toBe(odd);
  });
  it("gives the surfaces the grounds the window wears, the sidebar a step back", () => {
    expect(glassVars("#282c34", "#1d2025", 0.75)).toEqual({
      "--ground-glass": "rgba(40, 44, 52, 0.75)",
      "--recessed-glass": "rgba(29, 32, 37, 0.75)",
    });
  });
});
