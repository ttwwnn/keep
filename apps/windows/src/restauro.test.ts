// What the core says about the restart and the conversations a drop stopped.
import { describe, expect, it } from "vitest";
import type { CoreAnswer } from "./api";
import { isQuietResume, isQuietRestore, readResume, readRestore, restoreLine, resumeLine } from "./restauro";

const answer = (body: Record<string, unknown>, exit = 0): CoreAnswer => ({ versao: 1, _saida: exit, ...body });

describe("the tabs back after a restart", () => {
  it("a round that brought tabs and conversations back says so", () => {
    const round = readRestore(
      answer({ restaurar: { origem: "keepd 4242 de 17", abas: 3, conversas: 1, falhas: [], motivo: "restaurado" }, retrato: { abas: 3, conversas: 1 } }),
    );
    expect(round).toEqual({ tabs: 3, conversations: 1, failures: [], reason: "restaurado", error: null });
    expect(isQuietRestore(round)).toBe(false);
    expect(restoreLine(round)).toBe("restauro: 3 aba(s), 1 conversa(s), restaurado");
  });

  it("the round every half minute, with nothing to do, is quiet", () => {
    for (const motivo of ["já restaurado neste keepd", "nada a restaurar", "outra restauração em andamento"]) {
      const round = readRestore(answer({ restaurar: { origem: null, abas: 0, conversas: 0, falhas: [], motivo } }));
      expect(isQuietRestore(round), motivo).toBe(true);
    }
  });

  it("what could not be done is not quiet, nor is a failure of the core", () => {
    const failed = readRestore(answer({ restaurar: { abas: 2, conversas: 0, falhas: ["A#3: pasta sumiu", "A#4: x"], motivo: "restaurado" } }));
    expect(restoreLine(failed)).toBe("restauro: 2 aba(s), 0 conversa(s), restaurado, falhas: A#3: pasta sumiu; A#4: x");
    const only = readRestore(answer({ restaurar: { abas: 0, conversas: 0, falhas: ["A#3: pasta sumiu"] } }));
    expect(isQuietRestore(only)).toBe(false);
    const broken = readRestore(answer({ erro: "o Keep não respondeu" }, 1));
    expect(broken.error).toBe("o Keep não respondeu");
    expect(isQuietRestore(broken)).toBe(false);
    expect(restoreLine(broken)).toBe("restauro: o Keep não respondeu");
  });

  it("a core that does not know the question is told so, not read as an empty round", () => {
    const older = readRestore(answer({ ok: false, motivo: "uso", detalhe: "o keep não entendeu o pedido" }, 2));
    expect(older.error).toBe("esta versão do keep não sabe restaurar as abas");
    expect(readRestore(answer({ ok: false, motivo: "uso", detalhe: "recusado" }, 1)).error).toBe("recusado");
  });

  it("an answer that is not what was expected counts for nothing", () => {
    const odd = readRestore(answer({ restaurar: { abas: "três", conversas: null, falhas: "x" } }));
    expect(odd).toEqual({ tabs: 0, conversations: 0, failures: [], reason: "", error: null });
  });
});

describe("the conversations a drop stopped", () => {
  it("a round that told some to continue says so", () => {
    const round = readResume(answer({ conversas: [], ignoradas: [], enviadas: 2, confirmadas: 1 }));
    expect(round).toEqual({ sent: 2, confirmed: 1, skipped: false, error: null });
    expect(isQuietResume(round)).toBe(false);
    expect(resumeLine(round)).toBe("continuar: 2 conversa(s) mandada(s) continuar, 1 confirmada(s)");
  });

  it("the usual round, with nothing stopped, or another one running, is quiet", () => {
    expect(isQuietResume(readResume(answer({ enviadas: 0, confirmadas: 0 })))).toBe(true);
    const other = readResume(answer({ motivo: "outra rodada em andamento" }));
    expect(other.skipped).toBe(true);
    expect(isQuietResume(other)).toBe(true);
  });

  it("a failure or an older core is not quiet", () => {
    const failed = readResume(answer({ erro: "o Keep não respondeu" }, 1));
    expect(isQuietResume(failed)).toBe(false);
    expect(resumeLine(failed)).toBe("continuar: o Keep não respondeu");
    expect(readResume(answer({ ok: false, motivo: "uso" }, 2)).error).toBe("esta versão do keep não sabe continuar as conversas");
  });
});
