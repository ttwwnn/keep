// The picker's lists: which tabs, in what order and under what names; what a
// query leaves; what can be done to a row; where a line of history is.
import { describe, expect, it, vi } from "vitest";
import type { SearchHit } from "../api";
import { tabKey } from "../state";
import { touch } from "../recent";
import { capsFor } from "./caps";
import { MAX_HISTORY, historyFor, plainAsScreen, rowsToSkip, topFor } from "./preview";
import { actionsFor, chordAction, filterActions } from "./actions";
import { columnOf, locateLine, matchSpan, type Cells, type Lines } from "./hits";
import { plainLetters } from "./letters";
import {
  commandItems,
  createItem,
  filterItems,
  firstSelectable,
  goToItems,
  grouped,
  hitItem,
  nextSelectable,
  openFolders,
  plainRows,
  previewName,
  recency,
  relativePlace,
  sectioned,
  segments,
  type GoToInput,
  type PickerItem,
  type TabFacts,
  type WorkspaceFacts,
} from "./items";

const tab = (workspace: string, id: number, position: number, over: Partial<TabFacts> = {}): TabFacts => ({
  workspace,
  tab: id,
  position,
  title: `aba ${id}`,
  panes: 1,
  program: "pwsh",
  cwd: "",
  busy: false,
  lastActive: 0,
  ...over,
});

const space = (name: string, tabs: TabFacts[], place = "", label = name): WorkspaceFacts => ({ name, label, place, tabs });

const input = (over: Partial<GoToInput>): GoToInput => ({
  workspaces: [],
  recent: [],
  here: null,
  directories: [],
  home: "C:\\Users\\ana",
  ...over,
});

const ids = (items: readonly PickerItem[]) => items.map((i) => i.id);

describe("the visits", () => {
  it("the latest comes first, once", () => {
    expect(touch(["a", "b", "c"], "c")).toEqual(["c", "a", "b"]);
    expect(touch([], "a")).toEqual(["a"]);
    expect(touch(["a", "b"], "z")).toEqual(["z", "a", "b"]);
  });
  it("a visit to the one in front changes nothing", () => {
    const visits = ["a", "b"];
    expect(touch(visits, "a")).toBe(visits);
  });
  it("only so many are kept", () => {
    expect(touch(["a", "b", "c"], "z", 3)).toEqual(["z", "a", "b"]);
  });
});

describe("how long ago", () => {
  const now = 10_000_000_000;
  it("in as few characters as will carry it", () => {
    expect(recency(now - 5_000, now)).toBe("agora");
    expect(recency(now - 5 * 60_000, now)).toBe("5m");
    expect(recency(now - 3 * 3_600_000, now)).toBe("3h");
    expect(recency(now - 2 * 86_400_000, now)).toBe("2d");
    expect(recency(now - 15 * 86_400_000, now)).toBe("2sem");
  });
  it("says nothing of what is not known, and a clock ahead is now", () => {
    expect(recency(0, now)).toBe("");
    expect(recency(now + 5_000, now)).toBe("agora");
  });
});

describe("a tab's place in its workspace", () => {
  it("below the workspace's folder, the part below it", () => {
    expect(relativePlace("C:\\www\\777leads\\api", "C:\\www\\777leads", "777leads")).toBe("api");
    expect(relativePlace("C:\\www\\777leads\\api\\v1\\", "C:\\www\\777leads", "777leads")).toBe("api/v1");
    expect(relativePlace("/home/ana/proj/api", "/home/ana/proj", "proj")).toBe("api");
  });
  it("in the folder itself, nothing", () => {
    expect(relativePlace("C:\\www\\777leads", "C:\\www\\777leads", "777leads")).toBeNull();
    expect(relativePlace("c:\\WWW\\777leads\\", "C:\\www\\777leads", "777leads")).toBeNull();
  });
  it("Windows folders are compared as Windows does, POSIX ones as they are", () => {
    expect(relativePlace("c:\\www\\proj\\Api", "C:\\WWW\\proj", "proj")).toBe("Api");
    expect(relativePlace("/home/ana/PROJ/api", "/home/ana/proj", "proj")).toBe("api");
  });
  it("a root of `proj` does not claim `project-two`", () => {
    expect(relativePlace("C:\\www\\project-two", "C:\\www\\proj", "proj")).toBe("project-two");
  });
  it("outside the folder, its last component; unknown, nothing", () => {
    expect(relativePlace("C:\\other\\thing", "C:\\www\\777leads", "777leads")).toBe("thing");
    expect(relativePlace("", "C:\\www\\777leads", "777leads")).toBeNull();
    expect(relativePlace("C:\\", "C:\\www\\777leads", "777leads")).toBe("C:");
  });
  it("without a folder to measure against, the last component says nothing when it repeats the name", () => {
    expect(relativePlace("C:\\www\\proj", "", "proj")).toBeNull();
    expect(relativePlace("C:\\www\\api", "", "proj")).toBe("api");
  });
});

describe("Ir para", () => {
  const workspaces = [
    space("alfa", [tab("alfa", 1, 0), tab("alfa", 2, 1), tab("alfa", 3, 2)]),
    space("beta", [tab("beta", 7, 0), tab("beta", 8, 1)]),
  ];

  it("leaves out the tab in front", () => {
    const items = goToItems(input({ workspaces, here: { workspace: "alfa", tab: 1 } }));
    expect(ids(items)).not.toContain(`run:${tabKey("alfa", 1)}`);
    expect(items).toHaveLength(4);
  });

  it("lists where you have been, latest first, then the rest by what was active last", () => {
    const ws = [
      space("alfa", [tab("alfa", 1, 0, { lastActive: 5 }), tab("alfa", 2, 1, { lastActive: 50 }), tab("alfa", 3, 2, { lastActive: 20 })]),
    ];
    const items = goToItems(input({ workspaces: ws, recent: [tabKey("alfa", 3), tabKey("alfa", 99)], here: null }));
    // The visited one, then 2 (active at 50) before 1 (at 5); a visit to a tab that is gone is skipped.
    expect(ids(items)).toEqual([`run:${tabKey("alfa", 3)}`, `run:${tabKey("alfa", 2)}`, `run:${tabKey("alfa", 1)}`]);
  });

  it("the first row is the last tab of another workspace", () => {
    const recent = [tabKey("alfa", 2), tabKey("alfa", 3), tabKey("beta", 8), tabKey("beta", 7), tabKey("alfa", 1)];
    const items = goToItems(input({ workspaces, recent, here: { workspace: "alfa", tab: 1 } }));
    expect(items[0].id).toBe(`run:${tabKey("beta", 8)}`);
    // Only moved up: the tabs of the workspace in front are still there, in their order.
    expect(ids(items).slice(1)).toEqual([`run:${tabKey("alfa", 2)}`, `run:${tabKey("alfa", 3)}`, `run:${tabKey("beta", 7)}`]);
  });

  it("with the list already starting elsewhere, nothing moves", () => {
    const recent = [tabKey("beta", 7), tabKey("alfa", 2)];
    const items = goToItems(input({ workspaces, recent, here: { workspace: "alfa", tab: 1 } }));
    expect(items[0].id).toBe(`run:${tabKey("beta", 7)}`);
  });

  it("names a tab by its workspace alone, with a position when there are siblings, a tail when it is below", () => {
    const ws = [
      space("proj", [tab("proj", 1, 0, { cwd: "C:\\w\\proj" }), tab("proj", 2, 1, { cwd: "C:\\w\\proj" }), tab("proj", 3, 2, { cwd: "C:\\w\\proj\\api" })], "C:\\w\\proj"),
      space("solo", [tab("solo", 9, 0)]),
    ];
    const items = goToItems(input({ workspaces: ws }));
    const context = (workspace: string, id: number) => items.find((i) => i.id === `run:${tabKey(workspace, id)}`)!.context;
    expect(context("proj", 3)).toBe("proj/api");
    // Same context twice: numbered by position, and only those.
    expect(context("proj", 1)).toBe("proj 1");
    expect(context("proj", 2)).toBe("proj 2");
    expect(context("solo", 9)).toBe("solo");
  });

  it("a position is only shown where there is a sibling in the list to be told apart from", () => {
    const ws = [space("proj", [tab("proj", 1, 0), tab("proj", 2, 1)])];
    const items = goToItems(input({ workspaces: ws, here: { workspace: "proj", tab: 1 } }));
    expect(items.map((i) => i.context)).toEqual(["proj"]);
  });

  it("writes the sidebar's name for the workspace, and keeps the daemon's for what it does", () => {
    const ws = [space("proj", [tab("proj", 1, 0)], "", "Clínica")];
    const [item] = goToItems(input({ workspaces: ws }));
    expect(item.context).toBe("Clínica");
    expect(item.workspace).toBe("proj");
  });

  it("says how many panes, what runs, where, and whether it is at work", () => {
    const ws = [space("proj", [tab("proj", 1, 0, { panes: 3, program: "claude", cwd: "C:\\w\\proj", busy: true, lastActive: 99, title: "Revisar" })])];
    const [item] = goToItems(input({ workspaces: ws }));
    expect(item).toMatchObject({ detail: "3 painéis", command: "claude", path: "C:\\w\\proj", busy: true, lastActive: 99, title: "Revisar" });
    expect(goToItems(input({ workspaces: [space("a", [tab("a", 1, 0)])] }))[0].detail).toBe("");
  });

  it("does not say a program's name twice", () => {
    const ws = [
      space("a", [
        tab("a", 1, 0, { title: "pwsh", program: "pwsh" }),
        tab("a", 2, 1, { title: "PowerShell.EXE", program: "powershell" }),
        tab("a", 3, 2, { title: "Revisar", program: "claude" }),
      ]),
    ];
    expect(goToItems(input({ workspaces: ws })).map((i) => i.command)).toEqual(["", "", "claude"]);
  });

  it("folders come after the tabs, minus the ones whose workspace exists", () => {
    const items = goToItems(
      input({
        workspaces: [space("Clinica", [tab("Clinica", 1, 0)])],
        directories: ["C:\\Users\\ana\\www\\clinica", "C:\\Users\\ana\\www\\api", "C:\\Users\\ana\\www\\api", "D:\\dados"],
      }),
    );
    const folders = items.filter((i) => i.kind.type === "destination");
    expect(folders.map((f) => f.title)).toEqual(["api", "dados"]);
    expect(folders[0]).toMatchObject({ detail: "~\\www\\api", path: "C:\\Users\\ana\\www\\api", workspace: "" });
    expect(items.findIndex((i) => i.kind.type === "destination")).toBeGreaterThan(items.findIndex((i) => i.kind.type === "running"));
  });
});

describe("the folders without zoxide", () => {
  it("are where the tabs are, once each, and home", () => {
    const ws = [
      space("a", [tab("a", 1, 0, { cwd: "C:\w\a" }), tab("a", 2, 1, { cwd: "c:\W\A" }), tab("a", 3, 2, { cwd: "" })]),
      space("b", [tab("b", 4, 0, { cwd: "C:\w\b" }), tab("b", 5, 1, { cwd: "C:\Users\ana" })]),
    ];
    expect(openFolders(ws, "C:\Users\ana")).toEqual(["C:\w\a", "C:\w\b", "C:\Users\ana"]);
    expect(openFolders([], "")).toEqual([]);
    expect(openFolders([], "/home/ana")).toEqual(["/home/ana"]);
  });
});

describe("a name that is not a workspace yet", () => {
  const ws = [space("alfa", [tab("alfa", 1, 0)])];
  it("is offered", () => {
    expect(createItem("  novo  ", ws)).toMatchObject({ kind: { type: "create", name: "novo" }, title: "Criar o workspace “novo”" });
  });
  it("is not offered for one that exists, or for nothing", () => {
    expect(createItem("alfa", ws)).toBeNull();
    expect(createItem("   ", ws)).toBeNull();
  });
});

describe("what a query leaves", () => {
  const items = goToItems(
    input({
      workspaces: [
        space("clinica", [tab("clinica", 1, 0, { title: "api", cwd: "C:\\w\\clinica\\api" }), tab("clinica", 2, 1, { title: "vim" })], "C:\\w\\clinica"),
        space("brokerfy", [tab("brokerfy", 5, 0, { title: "clin deploy" })]),
      ],
      directories: ["C:\\Users\\ana\\www\\acaua"],
    }),
  );

  it("nothing typed, everything", () => {
    expect(filterItems(items, "  ").kept).toHaveLength(items.length);
  });

  it("the context column answers first, so a workspace's tabs gather", () => {
    const { kept, lit } = filterItems(items, "clin");
    // `clinica` (context) wins over the `clin deploy` title of another workspace.
    expect(kept.map((i) => i.workspace).slice(0, 2)).toEqual(["clinica", "clinica"]);
    expect(lit.get(kept[0].id)).toMatchObject({ column: "context" });
    expect(kept.at(-1)!.title).toBe("clin deploy");
    expect(lit.get(kept.at(-1)!.id)).toMatchObject({ column: "title", ranges: [[0, 4]] });
  });

  it("says which letters, and on which column", () => {
    const { lit } = filterItems(items, "bkf");
    const row = items.find((i) => i.workspace === "brokerfy")!;
    expect(lit.get(row.id)).toEqual({ column: "context", ranges: [[0, 1], [3, 4], [6, 7]] });
  });

  it("a folder is found by its path", () => {
    const { kept, lit } = filterItems(items, "www/acaua");
    expect(kept.map((i) => i.title)).toEqual(["acaua"]);
    expect(lit.get(kept[0].id)).toMatchObject({ column: "detail", ranges: [[2, 11]] });
    // Typed with either slash.
    expect(filterItems(items, "www\\acaua").kept.map((i) => i.title)).toEqual(["acaua"]);
  });

  it("what does not match is gone; rows that tie keep the list's order", () => {
    expect(filterItems(items, "zzzz").kept).toHaveLength(0);
    const { kept } = filterItems(items, "clinica");
    expect(kept.map((i) => i.id)).toEqual(items.filter((i) => i.workspace === "clinica").map((i) => i.id));
  });
});

describe("the rows the list draws", () => {
  const [a, b] = goToItems(input({ workspaces: [space("a", [tab("a", 1, 0)]), space("b", [tab("b", 2, 0)])] }));
  const [folder] = goToItems(input({ directories: ["C:\\x\\pasta"] }));
  const made = createItem("novo", [])!;

  it("terminals and folders under their headings, a made-up workspace last", () => {
    const rows = sectioned([folder, a, made, b]);
    expect(rows.map((r) => (r.type === "heading" ? `# ${r.text}` : r.item.id))).toEqual([
      "# terminais",
      a.id,
      b.id,
      "# abrir em",
      folder.id,
      made.id,
    ]);
  });

  it("a heading goes with its section", () => {
    expect(sectioned([a]).filter((r) => r.type === "heading").map((r) => r.type === "heading" && r.text)).toEqual(["terminais"]);
    expect(sectioned([folder]).filter((r) => r.type === "heading").map((r) => r.type === "heading" && r.text)).toEqual(["abrir em"]);
    expect(sectioned([])).toEqual([]);
  });

  it("the palette's commands under their groups, one heading per run", () => {
    const rows = grouped(commandItems());
    const headings = rows.filter((r) => r.type === "heading").map((r) => r.type === "heading" && r.text);
    expect(headings).toEqual(["janela", "workspace", "aparência"]);
    expect(rows[0]).toEqual({ type: "heading", text: "janela" });
  });

  it("a search's rows have no headings", () => {
    expect(plainRows([a, b]).every((r) => r.type === "item")).toBe(true);
  });
});

describe("the selection", () => {
  const [a, b, c] = goToItems(input({ workspaces: [space("a", [tab("a", 1, 0), tab("a", 2, 1), tab("a", 3, 2)])] }));
  const rows = sectioned([a, b, c]);
  const dim: PickerItem = { ...b, id: "cmd:dim", disabled: true };

  it("starts on the first row that is a row", () => {
    expect(firstSelectable(rows)).toBe(1);
    expect(firstSelectable([])).toBe(-1);
  });

  it("steps over headings, and stops at the ends", () => {
    expect(nextSelectable(rows, 1, 1)).toBe(2);
    expect(nextSelectable(rows, 3, 1)).toBe(3);
    expect(nextSelectable(rows, 1, -1)).toBe(1);
    expect(nextSelectable(rows, 0, 1)).toBe(1);
  });

  it("never rests on a command that cannot run", () => {
    const list = plainRows([a, dim, c]);
    expect(nextSelectable(list, 0, 1)).toBe(2);
    expect(nextSelectable(list, 2, -1)).toBe(0);
    expect(firstSelectable(plainRows([dim, c]))).toBe(1);
  });
});

describe("the palette's commands", () => {
  it("the Mac's seventeen and Windows' three, in three groups", async () => {
    vi.resetModules();
    const { commandItems: items } = await import("./items");
    const list = items();
    expect(list).toHaveLength(20);
    const ofGroup = (g: string) => list.filter((i) => i.context === g).length;
    expect([ofGroup("janela"), ofGroup("workspace"), ofGroup("aparência")]).toEqual([8, 6, 6]);
    for (const title of ["Mudar o tema…", "Mudar a fonte…", "Redefinir o tamanho do texto", "Usar o tema do Keep", "Encerrar este workspace", "Configurações", "Tela cheia", "Colar imagem"]) {
      expect(list.map((i) => i.title)).toContain(title);
    }
  });

  it("a command nothing is bound to is dim until something is", async () => {
    vi.resetModules();
    const { registerCommand } = await import("../commands");
    const { commandItems: items } = await import("./items");
    const dim = () => items().filter((i) => i.disabled).map((i) => i.id);
    expect(dim()).toContain("cmd:newWindow");
    registerCommand("newWindow", { run: () => {} });
    expect(dim()).not.toContain("cmd:newWindow");
    let allowed = false;
    registerCommand("closePane", { run: () => {}, enabled: () => allowed });
    expect(dim()).toContain("cmd:closePane");
    allowed = true;
    expect(dim()).not.toContain("cmd:closePane");
  });

  it("running one runs what is bound, and only if it can", async () => {
    vi.resetModules();
    const { registerCommand, runCommand } = await import("../commands");
    const ran: string[] = [];
    expect(runCommand("newTab")).toBe(false);
    registerCommand("newTab", { run: () => ran.push("first") });
    registerCommand("newTab", { run: () => ran.push("second") });
    expect(runCommand("newTab")).toBe(true);
    expect(ran).toEqual(["second"]);
    registerCommand("killWorkspace", { run: () => ran.push("kill"), enabled: () => false });
    expect(runCommand("killWorkspace")).toBe(false);
    expect(ran).toEqual(["second"]);
  });

  it("a default never takes a command back, whichever came first", async () => {
    vi.resetModules();
    const { registerCommand, registerDefault, runCommand } = await import("../commands");
    const ran: string[] = [];
    // The owner bound it before the window's defaults were made.
    registerCommand("chooseTheme", { run: () => ran.push("owner") });
    registerDefault("chooseTheme", { run: () => ran.push("default") });
    // And one it binds after.
    registerDefault("chooseFont", { run: () => ran.push("default font") });
    registerCommand("chooseFont", { run: () => ran.push("owner font") });
    runCommand("chooseTheme");
    runCommand("chooseFont");
    expect(ran).toEqual(["owner", "owner font"]);
  });

  it("the destructive ones are said so, and the themes and fonts open lists", async () => {
    vi.resetModules();
    const { COMMANDS } = await import("../commands");
    expect(COMMANDS.filter((c) => c.destructive).map((c) => c.id).sort()).toEqual(["closePane", "closeTab", "killWorkspace", "removeWorkspace"]);
    expect(COMMANDS.filter((c) => c.opens).map((c) => [c.id, c.opens])).toEqual([
      ["chooseTheme", "themes"],
      ["chooseFont", "fonts"],
    ]);
  });

  it("a list that is registered is the one asked for", async () => {
    vi.resetModules();
    const { registerCatalog, catalogOf } = await import("../commands");
    const view = () => null;
    expect(catalogOf("themes")).toBeUndefined();
    registerCatalog("themes", view);
    expect(catalogOf("themes")).toBe(view);
    expect(catalogOf("fonts")).toBeUndefined();
  });
});

describe("what can be done to a row", () => {
  const [running] = goToItems(input({ workspaces: [space("proj", [tab("proj", 1, 0, { cwd: "C:\\w\\proj" })])] }));
  const [bare] = goToItems(input({ workspaces: [space("proj", [tab("proj", 1, 0)])] }));
  const [folder] = goToItems(input({ directories: ["C:\\x\\pasta"] }));
  const hit: SearchHit = { workspace: "proj", tab: 4, line: 9, total: 20, text: "erro: falhou", matchStart: 0, matchLen: 4, before: [], after: [] };

  const titles = (item: PickerItem) => actionsFor(item).map((e) => e.title);

  it("a terminal: go there, a tab beside it, show it, copy where it is, close it", () => {
    expect(titles(running)).toEqual([
      "Ir para este terminal",
      "Nova aba em proj",
      "Mostrar no Explorer",
      "Copiar o caminho da pasta",
      "Fechar este terminal",
    ]);
    const entries = actionsFor(running);
    expect(entries.at(-1)).toMatchObject({ destructive: true, startsGroup: true, keys: ["Ctrl", "D"] });
    expect(entries[1].startsGroup).toBe(true);
  });

  it("a terminal that has not said where it is has no folder to show or copy", () => {
    expect(titles(bare)).toEqual(["Ir para este terminal", "Nova aba em proj", "Fechar este terminal"]);
  });

  it("a folder: open a workspace in it, show it, copy it", () => {
    expect(titles(folder)).toEqual(["Abrir um workspace aqui", "Mostrar no Explorer", "Copiar o caminho"]);
  });

  it("a line of history: go to it, copy it", () => {
    const item = hitItem(hit, 4, "proj › aba 1");
    expect(titles(item)).toEqual(["Ir para esta linha", "Copiar a linha"]);
  });

  it("a command: itself, in red when it does not undo", () => {
    const commands = commandItems();
    const kill = commands.find((i) => i.id === "cmd:killWorkspace")!;
    expect(actionsFor(kill)).toEqual([expect.objectContaining({ title: "Encerrar este workspace", destructive: true, icon: "kill" })]);
    const bigger = commands.find((i) => i.id === "cmd:fontBigger")!;
    expect(actionsFor(bigger)[0].destructive).toBeFalsy();
  });

  it("typing narrows them", () => {
    const entries = actionsFor(running);
    expect(filterActions(entries, "COPIAR").map((e) => e.title)).toEqual(["Copiar o caminho da pasta"]);
    expect(filterActions(entries, "  ")).toHaveLength(entries.length);
    expect(filterActions(entries, "zzz")).toEqual([]);
  });

  it("the chords name the same actions without the panel", () => {
    expect(chordAction(running, "reveal")).toBe("reveal");
    expect(chordAction(running, "copy")).toBe("copyPath");
    expect(chordAction(bare, "reveal")).toBeNull();
    expect(chordAction(bare, "copy")).toBeNull();
    expect(chordAction(folder, "copy")).toBe("copyPath");
    expect(chordAction(hitItem(hit, 4, "x"), "copy")).toBe("copyText");
    expect(chordAction(hitItem(hit, 4, "x"), "reveal")).toBeNull();
    expect(chordAction(commandItems()[0], "copy")).toBeNull();
  });
});

describe("a line of history", () => {
  it("its match is a byte range, and JavaScript cuts by UTF-16", () => {
    expect(matchSpan("erro: falhou", 0, 4)).toEqual([0, 4]);
    // "ação" is five bytes: the match "o" after "aç" and "ã" starts at byte 5.
    expect(matchSpan("ação final", 6, 5)).toEqual([4, 9]);
    // An emoji is four bytes and two units.
    expect(matchSpan("😀 ok", 5, 2)).toEqual([3, 5]);
  });
  it("a range that does not fit the line is none", () => {
    expect(matchSpan("abc", 2, 5)).toBeNull();
    expect(matchSpan("abc", 0, 0)).toBeNull();
    expect(matchSpan("abc", -1, 2)).toBeNull();
  });
  it("is a row with its place and its line number from 1", () => {
    const hit: SearchHit = { workspace: "proj", tab: 4, line: 9, total: 20, text: "ação final", matchStart: 6, matchLen: 5, before: ["a"], after: ["b"] };
    const item = hitItem(hit, 2, "proj › vim");
    expect(item).toMatchObject({ title: "ação final", detail: "proj › vim:10", workspace: "proj", kind: { type: "hit", root: 2, span: [4, 9] } });
    expect(hitItem({ ...hit, text: "" }, 2, "x").title).toBe(" ");
  });
});

/** A terminal's buffer from its text. */
const buffer = (...lines: string[]): Lines => ({ length: lines.length, text: (i) => lines[i] ?? "" });

describe("where a line is in the terminal", () => {
  // The daemon's copy: ten lines, counted from the oldest.
  const history = ["l0", "l1", "l2", "l3", "l4", "erro aqui", "l6", "l7", "l8", "l9"];
  const hit = { text: "erro aqui", line: 5, total: 10 };

  it("counted back from the newest line", () => {
    expect(locateLine(buffer(...history), hit)).toEqual({ index: 5, exact: true });
  });

  it("a terminal that trimmed the oldest lines still finds it", () => {
    expect(locateLine(buffer(...history.slice(3)), hit)).toEqual({ index: 2, exact: true });
  });

  it("the blank rows under the output are not counted", () => {
    expect(locateLine(buffer(...history, "", "", ""), hit)).toEqual({ index: 5, exact: true });
  });

  it("a terminal that printed more since is looked through for the line", () => {
    const later = [...history, "m1", "m2", "m3"];
    expect(locateLine(buffer(...later), hit)).toEqual({ index: 5, exact: true });
  });

  it("the closest one wins when a line is there twice", () => {
    const twice = ["erro aqui", "x", "y", "erro aqui", "z", "w", "v", "u", "t", "s"];
    expect(locateLine(buffer(...twice), { text: "erro aqui", line: 4, total: 10 })).toEqual({ index: 3, exact: true });
  });

  it("trailing blanks make no difference", () => {
    expect(locateLine(buffer(...history.map((l) => `${l}   `)), hit)).toEqual({ index: 5, exact: true });
  });

  it("when the text is nowhere near, the count stands, flagged as a guess", () => {
    expect(locateLine(buffer(...history.map((l) => l.toUpperCase())), hit)).toEqual({ index: 5, exact: false });
  });

  it("older than anything the terminal kept: its oldest line", () => {
    expect(locateLine(buffer("a", "b"), { text: "velho", line: 0, total: 50 })).toEqual({ index: 0, exact: false });
  });

  it("an empty terminal has no line", () => {
    expect(locateLine(buffer("", "  "), hit)).toBeNull();
    expect(locateLine(buffer(), hit)).toBeNull();
  });
});

describe("keys as caps", () => {
  it("Windows writes them out, a Mac draws them", () => {
    expect(capsFor(["Ctrl", "Shift", "K"], false)).toEqual(["Ctrl", "Shift", "K"]);
    expect(capsFor(["Ctrl", "Shift", "Alt", "K", "↩"], true)).toEqual(["⌘", "⇧", "⌥", "K", "↩"]);
  });
});

describe("columns", () => {
  /** A row of cells from its text: a wide letter is a cell of width 2 and an empty one of width 0 after it. */
  const row = (text: string): Cells => {
    const cells: { getChars(): string; getWidth(): number }[] = [];
    for (const char of text) {
      const wide = (char.codePointAt(0) ?? 0) > 0x3000;
      cells.push({ getChars: () => char, getWidth: () => (wide ? 2 : 1) });
      if (wide) cells.push({ getChars: () => "", getWidth: () => 0 });
    }
    return { length: cells.length, getCell: (x) => cells[x] };
  };

  it("plain text: the offset is the column", () => {
    expect(columnOf(row("abcdef"), 0)).toBe(0);
    expect(columnOf(row("abcdef"), 3)).toBe(3);
    expect(columnOf(row("abcdef"), 6)).toBe(6);
    expect(columnOf(row("abcdef"), 99)).toBe(6);
  });

  it("a wide letter takes two columns and the half after it nothing", () => {
    // あ is at column 0 and 1; b is the second letter and sits at column 2.
    expect(columnOf(row("あbc"), 1)).toBe(2);
    expect(columnOf(row("あbc"), 3)).toBe(4);
  });
});

describe("what a preview is called", () => {
  it("the workspace and the tab, or the tab's folder when it has no title", () => {
    const [titled] = goToItems(input({ workspaces: [space("proj", [tab("proj", 1, 0, { title: "api", cwd: "C:\\w" })], "C:\\w")] }));
    expect(previewName(titled)).toBe("proj  ·  api");
    const [untitled] = goToItems(input({ workspaces: [space("proj", [tab("proj", 1, 0, { title: "", cwd: "C:\\w\\proj" })], "C:\\w\\proj")] }));
    expect(previewName(untitled)).toBe("proj  ·  C:\\w\\proj");
  });
  it("a folder by its path, a hit by where it is, a command by its name", () => {
    const [folder] = goToItems(input({ directories: ["C:\\Users\\ana\\www\\api"] }));
    expect(previewName(folder)).toBe("~\\www\\api");
    expect(previewName(hitItem({ workspace: "p", tab: 1, line: 3, total: 9, text: "x", matchStart: 0, matchLen: 1, before: [], after: [] }, 1, "p › vim"))).toBe("p › vim:4");
    expect(previewName(commandItems()[0])).toBe("Alternar a barra lateral");
  });
});

describe("fitting a screen in its frame", () => {
  it("what the plain preview says, as a terminal reads it", () => {
    expect(plainAsScreen("a\nb\r\nc")).toBe("a\r\nb\r\nc");
  });
  it("a screen that fits stays where it is", () => {
    // 24 rows of 14 px in a frame of 400 px: 28 rows are shown.
    expect(rowsToSkip(10, 14, 400)).toBe(0);
    expect(rowsToSkip(27, 14, 400)).toBe(0);
    expect(rowsToSkip(-1, 14, 400)).toBe(0);
  });
  it("one that does not loses its top rows, to show its last", () => {
    expect(rowsToSkip(40, 14, 400)).toBe(13);
    expect(rowsToSkip(28, 14, 400)).toBe(1);
    expect(rowsToSkip(5, 0, 400)).toBe(0);
  });
  it("a line is put in the middle of the view, and not past either end", () => {
    expect(topFor(100, 20, 500)).toBe(90);
    expect(topFor(3, 20, 500)).toBe(0);
    expect(topFor(495, 20, 500)).toBe(485);
    expect(topFor(499, 20, 120)).toBe(120);
    expect(topFor(5, 20, 0)).toBe(0);
  });
  it("a hit keeps the history it is in, a plain screen none", () => {
    const base = { workspace: "p", pane: 1, cols: 80, rows: 24 };
    expect(historyFor(base, 24)).toBe(0);
    expect(historyFor({ ...base, hit: { text: "x", line: 3, total: 100 } }, 24)).toBe(124);
    // The grid the daemon holds it on now, not the one the list said.
    expect(historyFor({ ...base, hit: { text: "x", line: 3, total: 100 } }, 43)).toBe(143);
    expect(historyFor({ ...base, hit: { text: "x", line: 3, total: 90_000 } }, 24)).toBe(MAX_HISTORY);
  });
});

describe("a matched string, cut for drawing", () => {
  it("lit where it matched, dark between", () => {
    expect(segments("Brokerfy", [[0, 1], [6, 8]])).toEqual([
      { text: "B", on: true },
      { text: "roker", on: false },
      { text: "fy", on: true },
    ]);
  });
  it("nothing matched, one dark run; nothing at all, none", () => {
    expect(segments("abc", undefined)).toEqual([{ text: "abc", on: false }]);
    expect(segments("abc", [])).toEqual([{ text: "abc", on: false }]);
    expect(segments("", [[0, 2]])).toEqual([]);
  });
  it("a range past the end is held to the text", () => {
    expect(segments("abc", [[2, 9]])).toEqual([
      { text: "ab", on: false },
      { text: "c", on: true },
    ]);
  });
});

describe("matching without the accents", () => {
  it("each accented letter is its plain one, and the length does not change", () => {
    expect(plainLetters("Configurações")).toBe("Configuracoes");
    expect(plainLetters("à ação ÇÃO ü")).toBe("a acao CAO u");
    expect(plainLetters("😀 ok")).toBe("😀 ok");
    for (const text of ["Configurações", "ação", "São Paulo", "naïve café"]) expect(plainLetters(text)).toHaveLength(text.length);
  });

  it("a letter written as a base and a mark keeps both, so the length holds", () => {
    const decomposed = "ac\u0327a\u0303o";
    expect(plainLetters(decomposed)).toHaveLength(decomposed.length);
    expect(plainLetters(decomposed).startsWith("ac")).toBe(true);
  });

  it("the palette finds Configurações by configuracoes, and Dividir à direita by dividir a dir", () => {
    const commands = commandItems();
    const title = (query: string) => filterItems(commands, query).kept.map((i) => i.title);
    expect(title("configuracoes")).toContain("Configurações");
    expect(title("dividir a dir")).toEqual(["Dividir à direita"]);
    expect(title("DIVIDIR À DIR")).toEqual(["Dividir à direita"]);
  });

  it("what lit up is a range of the text as written", () => {
    const { kept, lit } = filterItems(commandItems(), "configuracoes");
    const row = kept.find((i) => i.title === "Configurações")!;
    expect(lit.get(row.id)).toEqual({ column: "title", ranges: [[0, 13]] });
  });

  it("the actions are narrowed the same way", () => {
    const entries = actionsFor(commandItems().find((i) => i.id === "cmd:settings")!);
    expect(filterActions(entries, "configuracoes").map((e) => e.title)).toEqual(["Configurações"]);
    const [running] = goToItems(input({ workspaces: [space("proj", [tab("proj", 1, 0, { cwd: "C:\\w" })])] }));
    expect(filterActions(actionsFor(running), "ir PARA").map((e) => e.title)).toEqual(["Ir para este terminal"]);
  });
});
