// What the Settings say of the claude-cli:// links, from what the core answers.
import { describe, expect, it } from "vitest";
import type { CoreAnswer } from "./api";
import { linksHint, readLinks } from "./deepLinks";

const answer = (fields: Record<string, unknown>): CoreAnswer => ({ versao: 1, _saida: 0, ...fields });

const item = (ok: boolean, descricao: string) => ({ ok, descricao });

describe("readLinks", () => {
  it("reads the state the core gives", () => {
    const reading = readLinks(
      answer({ ok: true, ligado: true, completo: true, app: "C:\\Keep\\Keep.exe", itens: [item(true, "a"), item(true, "b")] }),
    );
    expect(reading).toEqual({ kind: "status", status: { on: true, complete: true, missing: [] } });
  });

  it("lists what is missing, in the core's words", () => {
    const reading = readLinks(answer({ ok: true, ligado: true, completo: false, itens: [item(true, "a"), item(false, "o Claude Code não refaz o registro")] }));
    expect(reading).toEqual({ kind: "status", status: { on: true, complete: false, missing: ["o Claude Code não refaz o registro"] } });
  });

  it("reads off", () => {
    const reading = readLinks(answer({ ok: true, ligado: false, completo: false, itens: [item(false, "x")] }));
    expect(reading).toMatchObject({ kind: "status", status: { on: false } });
  });

  it("is unavailable without a core, or with one that does not know the command", () => {
    expect(readLinks(null)).toEqual({ kind: "unavailable" });
    expect(readLinks(answer({ ok: false, motivo: "uso", detalhe: "o keep não entendeu o pedido", _saida: 2 }))).toEqual({ kind: "unavailable" });
    expect(readLinks(answer({}))).toEqual({ kind: "unavailable" });
    expect(readLinks(answer({ ligado: "sim", itens: [] }))).toEqual({ kind: "unavailable" });
    expect(readLinks(answer({ ligado: true, itens: "x" }))).toEqual({ kind: "unavailable" });
  });

  it("says why the core refused", () => {
    expect(readLinks(answer({ ok: false, motivo: "erro", detalhe: "settings.json não é um objeto JSON", _saida: 1 }))).toEqual({
      kind: "error",
      message: "settings.json não é um objeto JSON",
    });
    expect(readLinks(answer({ ok: false, motivo: "erro", _saida: 1 }))).toMatchObject({ kind: "error" });
  });

  it("ignores items that are not as the core writes them", () => {
    const reading = readLinks(answer({ ligado: true, completo: true, itens: [null, "x", { ok: false }, { descricao: "y" }, item(false, "z")] }));
    expect(reading).toMatchObject({ kind: "status", status: { missing: ["z"] } });
  });
});

describe("linksHint", () => {
  it("explains the switch before the first look, when on and when off", () => {
    const explanation = linksHint(null);
    expect(explanation).toContain("sem administrador");
    expect(explanation).toContain("devolve");
    expect(linksHint({ kind: "status", status: { on: false, complete: false, missing: [] } })).toBe(explanation);
    expect(linksHint({ kind: "status", status: { on: true, complete: true, missing: [] } })).toBe(explanation);
  });

  it("says what is missing when on but not complete", () => {
    const text = linksHint({ kind: "status", status: { on: true, complete: false, missing: ["a", "b"] } });
    expect(text).toContain("incompleto: a; b");
  });

  it("says the core's complaint and the missing core", () => {
    expect(linksHint({ kind: "error", message: "deu errado" })).toBe("deu errado");
    expect(linksHint({ kind: "unavailable" })).toContain("não tem");
  });
});
