// The names given to tabs, in the file the core shares with the window: which
// are the window's to write, which are the core's to be taken.
import { describe, expect, it } from "vitest";
import {
  daemonStartOf,
  foreignNames,
  knownFor,
  namesFileOf,
  planNames,
  readNamesFile,
  sameDaemon,
  sameNames,
  sameStart,
  type NamesFile,
} from "./names";

const file = (tabs: Record<string, string>, start: number | null = 1791234567.89): NamesFile => ({
  daemonStart: start,
  tabs,
  workspaces: {},
});

describe("which daemon", () => {
  it("its start is the one it says, in seconds", () => {
    expect(daemonStartOf("4242-1791234567890")).toBe(1791234567.89);
    expect(daemonStartOf("7-1000")).toBe(1);
  });

  it("a daemon that could not say stands in by the time its process was created, which stamps nothing", () => {
    expect(daemonStartOf("4242-133700000000000000")).toBeNull();
    expect(daemonStartOf("")).toBeNull();
    expect(daemonStartOf("4242")).toBeNull();
    expect(daemonStartOf("4242-0")).toBeNull();
    expect(daemonStartOf("4242-abc")).toBeNull();
  });

  it("the same to the hundredth of a second, as the core compares", () => {
    expect(sameStart(1791234567.89, 1791234567.895)).toBe(true);
    expect(sameStart(1791234567.89, 1791234568)).toBe(false);
    expect(sameStart(null, 1)).toBe(false);
    expect(sameStart(null, null)).toBe(false);
  });

  it("one daemon told two ways is one daemon; two daemons are not", () => {
    expect(sameDaemon("4242-1791234567890", "4242-1791234567890")).toBe(true);
    expect(sameDaemon("4242-1791234567890", "4242-1791234999999")).toBe(false);
    expect(sameDaemon("4243-1791234567890", "4242-1791234567890")).toBe(false);
    // What an earlier window kept — the process's creation time — and the same process now.
    expect(sameDaemon("4242-133700000000000000", "4242-1791234567890")).toBe(true);
    expect(sameDaemon("4243-133700000000000000", "4242-1791234567890")).toBe(false);
    // Nothing kept is nothing known.
    expect(sameDaemon("", "4242-1791234567890")).toBe(false);
  });
});

describe("the file as the core writes it", () => {
  it("is read as it comes, tolerant of what it lacks", () => {
    const text = '{"daemonStart": 1791234567.89, "tabs": {"a\\u001f3": "Minha conversa", "b\\u001f1": "  "}, "workspaces": {"a": "Casa"}}';
    expect(readNamesFile(text)).toEqual({
      daemonStart: 1791234567.89,
      tabs: { "a\u001f3": "Minha conversa" },
      workspaces: { a: "Casa" },
    });
    expect(readNamesFile("{}")).toEqual({ daemonStart: null, tabs: {}, workspaces: {} });
    expect(readNamesFile('﻿{"daemonStart": 1}')?.daemonStart).toBe(1);
    expect(readNamesFile("não é JSON")).toBeNull();
    expect(readNamesFile("[]")).toBeNull();
    expect(readNamesFile(null)).toBeNull();
  });

  it("is made of the window's names, the tab keys with the core's separator", () => {
    const made = namesFileOf(
      { tabNames: { "a\u00003": "Minha conversa", "b c\u00001": "Outra", "x\u00002": " " }, workspaceNames: { a: "Casa" } },
      1791234567.89,
    );
    expect(made).toEqual({
      daemonStart: 1791234567.89,
      tabs: { "a\u001f3": "Minha conversa", "b c\u001f1": "Outra" },
      workspaces: { a: "Casa" },
    });
    // Written and read back it is the same, down to the control character in the key.
    expect(readNamesFile(JSON.stringify(made))).toEqual(made);
    expect(JSON.stringify(made)).toContain("a\\u001f3");
  });

  it("says when two are the same", () => {
    const a = file({ "a\u001f1": "Um" });
    expect(sameNames(a, file({ "a\u001f1": "Um" }, 1791234567.891))).toBe(true);
    expect(sameNames(a, file({ "a\u001f1": "Dois" }))).toBe(false);
    expect(sameNames(a, file({}))).toBe(false);
    expect(sameNames(a, file({ "a\u001f1": "Um" }, 1791234999))).toBe(false);
    expect(sameNames(a, null)).toBe(false);
    expect(sameNames(null, null)).toBe(true);
  });
});

describe("what the window takes from the file", () => {
  it("the first time everything in it is news", () => {
    expect(foreignNames(file({ "ws\u001f2": "Um", "outro ws\u001f10": "Dez" }), null)).toEqual([
      { workspace: "ws", tab: 2, name: "Um" },
      { workspace: "outro ws", tab: 10, name: "Dez" },
    ]);
  });

  it("afterwards only what changed since the window last knew it: a tab named, a name replaced", () => {
    const known = file({ "ws\u001f2": "Um", "ws\u001f3": "Três" });
    const now = file({ "ws\u001f2": "Um", "ws\u001f3": "Três de novo", "ws\u001f4": "Quatro" });
    expect(foreignNames(now, known)).toEqual([
      { workspace: "ws", tab: 3, name: "Três de novo" },
      { workspace: "ws", tab: 4, name: "Quatro" },
    ]);
    expect(foreignNames(known, known)).toEqual([]);
  });

  it("a name the window took off is not brought back by the file repeating it", () => {
    const known = file({ "ws\u001f2": "Um" });
    // The window cleared it and has not written yet: the file still says it, as the window last knew it.
    expect(foreignNames(file({ "ws\u001f2": "Um" }), known)).toEqual([]);
  });

  it("keys that are not a workspace and a tab number are ignored", () => {
    expect(foreignNames(file({ semseparador: "x", "ws\u001fabc": "y", "\u001f3": "z", "ws\u001f0": "w" }), null)).toEqual([]);
  });
});

describe("a round of names", () => {
  const START = 1791234567.89;
  const state = (tabNames: Record<string, string>) => ({ tabNames, workspaceNames: {} });
  const text = (f: NamesFile) => JSON.stringify(f);

  it("what the window knew of the file counts only for the daemon that wrote it", () => {
    const known = file({ "ws\u001f1": "Um" }, 1.5);
    expect(knownFor(known, 1.5)).toBe(known);
    expect(knownFor(known, 2)).toBeNull();
    expect(knownFor(null, 2)).toBeNull();
  });

  it("the window's names go to a file that has none of them, and nothing is written twice", () => {
    const mine = state({ "ws\u00001": "Um" });
    const first = planNames({ state: mine, daemonStart: START, text: null, known: null });
    expect(first.adopt).toEqual([]);
    expect(first.write).toEqual(file({ "ws\u001f1": "Um" }, START));
    const next = planNames({ state: mine, daemonStart: START, text: null, known: first.write });
    expect(next.write).toBeNull();
  });

  it("a restart: the names the core gave the tabs it made come back, though the window held others of the daemon before", () => {
    // The old daemon's file, as the window last wrote it; the new daemon's, with the names the picture had.
    const before = file({ "ws\u001f2": "Minha conversa" }, 1791000000);
    const arrived = file({ "ws\u001f2": "Minha conversa", "ws\u001f3": "Outra" }, START);
    // The window cleared its own (a new daemon: its tab numbers are not the old one's).
    const plan = planNames({ state: state({}), daemonStart: START, text: text(arrived), known: before });
    expect(plan.adopt).toEqual([
      { workspace: "ws", tab: 2, name: "Minha conversa" },
      { workspace: "ws", tab: 3, name: "Outra" },
    ]);
    // With them in, the window has nothing to add to the file: it is not rewritten over what the core wrote.
    expect(plan.write).toBeNull();
    expect(plan.known).toEqual(arrived);
  });

  it("names the window has that the core's file lacks are added to it, the core's kept", () => {
    const arrived = file({ "ws\u001f2": "Do núcleo" }, START);
    const plan = planNames({ state: state({ "ws\u00001": "Meu" }), daemonStart: START, text: text(arrived), known: null });
    expect(plan.adopt).toEqual([{ workspace: "ws", tab: 2, name: "Do núcleo" }]);
    expect(plan.write).toEqual(file({ "ws\u001f1": "Meu", "ws\u001f2": "Do núcleo" }, START));
  });

  it("a name the person took off stays off, and a name the core replaced wins", () => {
    const known = file({ "ws\u001f1": "Um", "ws\u001f2": "Dois" }, START);
    // The window cleared tab 1 (the file still says it, as known), and the core renamed tab 2.
    const arrived = file({ "ws\u001f1": "Um", "ws\u001f2": "Dois (nome novo)" }, START);
    const plan = planNames({ state: state({ "ws\u00002": "Dois" }), daemonStart: START, text: text(arrived), known });
    expect(plan.adopt).toEqual([{ workspace: "ws", tab: 2, name: "Dois (nome novo)" }]);
    expect(plan.write).toEqual(file({ "ws\u001f2": "Dois (nome novo)" }, START));
  });

  it("a file of another daemon is not taken, and is written over", () => {
    const old = file({ "ws\u001f1": "De outro" }, 1791000000);
    const plan = planNames({ state: state({ "ws\u00001": "Meu" }), daemonStart: START, text: text(old), known: null });
    expect(plan.adopt).toEqual([]);
    expect(plan.write).toEqual(file({ "ws\u001f1": "Meu" }, START));
    // A file that is not one: the same.
    expect(planNames({ state: state({}), daemonStart: START, text: "lixo", known: null }).write).toEqual(file({}, START));
  });
});
