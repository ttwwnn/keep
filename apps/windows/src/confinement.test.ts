// The folder a workspace's AI is held to: the file as read, the button's
// state and words, and the folders that cannot hold the AI.
import { describe, expect, it } from "vitest";
import {
  CONFINEMENT_ENFORCED,
  LABELS,
  LATER_SESSIONS,
  chosen,
  confineLook,
  contextChoices,
  elide,
  heldChoices,
  normalizeConfinements,
  refusal,
  sameConfinements,
  tooBroad,
} from "./confinement";

describe("the file", () => {
  it("keeps a name with a folder and drops the rest, as the core and the macOS app read it", () => {
    expect(normalizeConfinements({ a: "C:\\x", b: "", c: 3, d: null, e: ["f"], g: "/y" })).toEqual({ a: "C:\\x", g: "/y" });
    expect(normalizeConfinements(null)).toEqual({});
    expect(normalizeConfinements("x")).toEqual({});
    expect(normalizeConfinements([1, 2])).toEqual({});
    expect(normalizeConfinements(undefined)).toEqual({});
  });

  it("tells a change from the same file read again", () => {
    expect(sameConfinements({}, {})).toBe(true);
    expect(sameConfinements({ a: "C:\\x" }, { a: "C:\\x" })).toBe(true);
    expect(sameConfinements({ a: "C:\\x" }, { a: "C:\\y" })).toBe(false);
    expect(sameConfinements({ a: "C:\\x" }, {})).toBe(false);
    expect(sameConfinements({ a: "C:\\x" }, { b: "C:\\x" })).toBe(false);
  });
});

describe("the button", () => {
  it("is open and offers the choice for a workspace that is free", () => {
    const look = confineLook("projeto", null);
    expect(look.held).toBe(false);
    expect(look.title.split("\n")[0]).toBe("Restringir a IA de projeto a uma pasta");
  });

  it("is shut and says where the AI is held when it is", () => {
    const look = confineLook("projeto", "D:\\work\\app");
    expect(look.held).toBe(true);
    expect(look.title.split("\n")[0]).toBe("A IA de projeto só acessa D:\\work\\app");
  });

  it("opens, when held, where the AI is held and the two ways out", () => {
    expect(heldChoices("D:\\work\\app")).toEqual([
      { id: "info", label: "A IA só acessa D:\\work\\app", disabled: true, help: "D:\\work\\app" },
      "separator",
      { id: "choose", label: "Trocar a pasta…" },
      { id: "release", label: "Liberar a IA" },
    ]);
  });

  it("has in the header's menu the choice, and the way out only when there is one", () => {
    expect(contextChoices(null)).toEqual(["separator", { id: "choose", label: "Restringir a IA a uma pasta…" }]);
    expect(contextChoices("D:\\work\\app")).toEqual([
      "separator",
      { id: "choose", label: "Restringir a IA a uma pasta…" },
      { id: "release", label: "Liberar a IA (hoje: D:\\work\\app)", help: "D:\\work\\app" },
    ]);
  });

  it("shortens a long path in the middle and keeps a short one", () => {
    expect(elide("D:\\work\\app")).toBe("D:\\work\\app");
    const long = "C:\\Users\\Ana\\Projetos\\clientes\\empresa-grande\\sistema-financeiro\\backend\\api";
    const short = elide(long);
    expect(short.length).toBe(48);
    expect(short.startsWith("C:\\Users\\Ana\\")).toBe(true);
    expect(short.endsWith("backend\\api")).toBe(true);
    expect(short).toContain("…");
    expect(heldChoices(long)[0]).toMatchObject({ help: long });
    expect(contextChoices(long)[2]).toMatchObject({ help: long });
    expect((contextChoices(long)[2] as { label: string }).label.length).toBeLessThan(80);
  });

  it("says in both states that a session already open keeps the access it had", () => {
    for (const folder of [null, "D:\\work\\app"]) {
      expect(confineLook("p", folder).title).toContain(LATER_SESSIONS);
    }
    expect(LATER_SESSIONS).toContain("já estão abertas mantêm o acesso que tinham");
  });

  it("words the choices as the macOS app does", () => {
    expect(LABELS.restrict).toBe("Restringir a IA a uma pasta…");
    expect(LABELS.change).toBe("Trocar a pasta…");
    expect(LABELS.release).toBe("Liberar a IA");
    expect(LABELS.releaseToday("D:\\app")).toBe("Liberar a IA (hoje: D:\\app)");
    expect(LABELS.dialogTitle("projeto")).toBe("Pasta que a IA de projeto pode acessar");
  });

  it("is off until the Windows CI proves the mechanism (KEEP_CONFINE_PROVA)", () => {
    expect(CONFINEMENT_ENFORCED).toBe(false);
  });
});

describe("folders that cannot hold the AI", () => {
  const home = "C:\\Users\\Ana";

  it("refuses the root of a drive, the home and what contains it", () => {
    for (const folder of ["C:\\", "D:\\", "D:", "C:\\Users", "C:\\Users\\Ana", "c:/users/ana/", "C:\\USERS\\ANA"]) {
      expect(tooBroad(folder, home), folder).not.toBeNull();
    }
    expect(tooBroad("C:\\", home)).toBe("a raiz de uma unidade");
    expect(tooBroad("C:\\Users\\Ana", home)).toBe("a pasta do seu usuário (ou uma que a contém)");
    expect(tooBroad("\\\\srv\\pasta", home)).toBe("a raiz de uma unidade");
    expect(tooBroad("/", "/Users/ana")).toBe("a raiz de uma unidade");
  });

  it("takes a project's folder, on the profile or elsewhere", () => {
    for (const folder of [
      "C:\\Users\\Ana\\Projetos\\app",
      "C:\\Users\\Ana\\Documents",
      "D:\\work",
      "D:\\work\\app",
      "\\\\srv\\pasta\\app",
      "C:\\Users\\Ana2",
    ]) {
      expect(tooBroad(folder, home), folder).toBeNull();
    }
  });

  it("reads a literal path as the plain one", () => {
    expect(tooBroad("\\\\?\\C:\\Users\\Ana", home)).not.toBeNull();
    expect(tooBroad("\\\\?\\C:\\", home)).not.toBeNull();
    expect(tooBroad("\\\\?\\C:\\work\\app", home)).toBeNull();
    expect(tooBroad("\\\\?\\UNC\\srv\\pasta", home)).not.toBeNull();
    expect(tooBroad("\\\\?\\UNC\\srv\\pasta\\app", home)).toBeNull();
  });

  it("does not take a folder for the home when the home is not known", () => {
    expect(tooBroad("C:\\Users\\Ana", "")).toBeNull();
  });

  it("says why, and what was chosen", () => {
    expect(refusal("C:\\", "a raiz de uma unidade")).toBe(
      "Escolha a pasta de um projeto: C:\\ é a raiz de uma unidade, e a IA não ficaria presa a nada.",
    );
    expect(chosen("projeto", "D:\\app")).toBe(`A IA de projeto só acessa D:\\app. ${LATER_SESSIONS}`);
  });
});
