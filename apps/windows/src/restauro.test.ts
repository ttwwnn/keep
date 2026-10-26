// What the core says about the restart and the conversations a drop stopped.
import { describe, expect, it } from "vitest";
import type { CoreAnswer } from "./api";
import {
  dormancyLine,
  isQuietResume,
  isQuietRestore,
  noTabNumbers,
  readResume,
  readRestore,
  remapTabNumbers,
  restoreLine,
  resumeLine,
  readSleepers,
  refusalOfSleep,
  sleepArgs,
  sleepTabArgs,
} from "./restauro";

const answer = (body: Record<string, unknown>, exit = 0): CoreAnswer => ({ versao: 1, _saida: exit, ...body });

describe("the tabs back after a restart", () => {
  it("a round that brought tabs and conversations back says so", () => {
    const round = readRestore(
      answer({ restaurar: { origem: "keepd 4242 de 17", abas: 3, conversas: 1, falhas: [], motivo: "restaurado" }, retrato: { abas: 3, conversas: 1 } }),
    );
    expect(round).toEqual({ tabs: 3, made: [], conversations: 1, failures: [], reason: "restaurado", error: null });
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
    expect(odd).toEqual({ tabs: 0, made: [], conversations: 0, failures: [], reason: "", error: null });
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

describe("the tabs the restore made", () => {
  it("are told with the number each had before", () => {
    const round = readRestore(
      answer({
        restaurar: {
          abas: 3,
          conversas: 0,
          falhas: [],
          motivo: "abas recriadas; as conversas seguem",
          criadas: [
            { workspace: "alfa", origem: 2, aba: 1 },
            { workspace: "alfa", origem: 5, aba: 2 },
            { workspace: "beta", origem: 1, aba: 1 },
            { workspace: "sem número", origem: 0, aba: 1 },
            "lixo",
          ],
        },
      }),
    );
    expect(round.made).toEqual([
      { workspace: "alfa", from: 2, to: 1 },
      { workspace: "alfa", from: 5, to: 2 },
      { workspace: "beta", from: 1, to: 1 },
    ]);
  });

  const made = [
    { workspace: "alfa", from: 2, to: 1 },
    { workspace: "alfa", from: 5, to: 2 },
    { workspace: "alfa", from: 9, to: 3 },
    { workspace: "beta", from: 1, to: 1 },
  ];

  it("carry the order the tabs were listed in, the last one shown and the one in front to their new numbers", () => {
    const was = {
      tabOrder: { alfa: [9, 2, 5], beta: [1] },
      lastTab: { alfa: 5, beta: 1 },
      active: { workspace: "alfa", tab: 9 },
    };
    expect(remapTabNumbers(was, made)).toEqual({
      tabOrder: { alfa: [3, 1, 2], beta: [1] },
      lastTab: { alfa: 2, beta: 1 },
      active: { workspace: "alfa", tab: 3 },
    });
  });

  it("leave behind what belonged to a tab that did not come back, and a workspace of none", () => {
    const was = {
      tabOrder: { alfa: [7, 2, 5], gama: [1, 2] },
      lastTab: { alfa: 7, gama: 1 },
      active: { workspace: "alfa", tab: 7 },
    };
    expect(remapTabNumbers(was, made)).toEqual({ tabOrder: { alfa: [1, 2] }, lastTab: {}, active: null });
    expect(remapTabNumbers({ tabOrder: {}, lastTab: {}, active: null }, made)).toEqual(noTabNumbers());
    expect(remapTabNumbers({ tabOrder: { alfa: [2] }, lastTab: {}, active: null }, [])).toEqual(noTabNumbers());
  });
});

describe("the conversations left alone", () => {
  it("spares the tab on screen in every window", () => {
    expect(sleepArgs([{ workspace: "Clinica", tab: 8 }, { workspace: "a/b", tab: 2 }])).toEqual([
      "restauro", "adormecer", "--vista", "Clinica/8", "--vista", "a/b/2",
    ]);
    expect(sleepArgs([])).toEqual(["restauro", "adormecer"]);
  });

  it("puts one tab to sleep on the person's word", () => {
    expect(sleepTabArgs("Clinica", 8)).toEqual(["restauro", "adormecer", "--agora", "Clinica", "--aba", "8"]);
    expect(sleepTabArgs("a/b", 2)).toEqual(["restauro", "adormecer", "--agora", "a/b", "--aba", "2"]);
  });

  it("reads the tabs asleep, and nothing from an answer that is not the list", () => {
    expect(readSleepers(answer({ abas: [{ workspace: "A", aba: 3 }, { workspace: "a/b", aba: 12 }] }))).toEqual([
      { workspace: "A", tab: 3 }, { workspace: "a/b", tab: 12 },
    ]);
    expect(readSleepers(answer({ abas: [] }))).toEqual([]);
    expect(readSleepers(answer({ abas: [{ workspace: "A" }, null, { aba: 1 }, { workspace: "B", aba: 2 }] }))).toEqual([{ workspace: "B", tab: 2 }]);
    expect(readSleepers(answer({}, 2)), "a core that does not know the question").toBeNull();
    expect(readSleepers(answer({ erro: "o Keep não respondeu" }))).toBeNull();
    expect(readSleepers(answer({ ok: false, abas: [{ workspace: "A", aba: 1 }] }))).toBeNull();
    expect(readSleepers(answer({ adormecidas: 1 })), "the round's answer is not the list").toBeNull();
  });

  it("tells why a tab refused to sleep, and nothing when it slept", () => {
    expect(refusalOfSleep(answer({ adormecidas: 1, abas: [{ aba: "A#2", situacao: "adormecida (parada há 0 h)" }] }))).toBeNull();
    expect(refusalOfSleep(answer({ adormecidas: 0, abas: [{ aba: "A#2", situacao: "parada há 0 h, mas trabalhando" }] }))).toBe(
      "parada há 0 h, mas trabalhando",
    );
    expect(refusalOfSleep(answer({ adormecidas: 0, abas: [] })), "no word on it").toBe("nada a hibernar nesta aba");
    expect(refusalOfSleep(answer({ motivo: "outra rodada em andamento" }))).toBe("outra rodada em andamento");
    expect(refusalOfSleep(answer({}, 2))).toBe("esta versão do keep não sabe hibernar a aba");
    expect(refusalOfSleep(answer({ erro: "o Keep não respondeu" }))).toBe("o Keep não respondeu");
  });

  it("says only what happened", () => {
    expect(dormancyLine(answer({ adormecidas: 0, abas: [] }))).toBeNull();
    expect(dormancyLine(answer({ acordadas: 0 }))).toBeNull();
    expect(dormancyLine(answer({}, 2)), "a core that does not know the question").toBeNull();
    expect(dormancyLine(answer({ adormecidas: 2, abas: [] }))).toBe("adormecer: 2 conversa(s) parada(s) fora da memória");
    expect(dormancyLine(answer({ acordadas: 1, falhas: ["A#2: a aba roda vim"] }))).toBe(
      "adormecer: 1 conversa(s) de volta, falhas: A#2: a aba roda vim",
    );
    expect(dormancyLine(answer({ erro: "o Keep não respondeu" }))).toBe("adormecer: o Keep não respondeu");
  });
});
