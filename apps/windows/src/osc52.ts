// OSC 52: a program writing to the system clipboard, or asking what is on it.
//
// The macOS app's terminal (Ghostty) lets a program write — Claude Code does,
// when it selects text with the mouse it has taken — and asks the person
// before a program reads (`clipboard-write = allow`, `clipboard-read = ask`).
// There is no selection clipboard on a Mac, and none on Windows: a write to
// the primary selection or to the selection (`s`, `p`) is dropped, and a read
// of either answers with the one clipboard there is, under the name asked.

import type { IDisposable, Terminal } from "@xterm/xterm";
import { preview, type Question } from "./paste";

/** Which clipboard the program named; `c` is the standard one and what any other letter means. */
export type Selection = "c" | "s" | "p";

export type Osc52 = { op: "write"; selection: Selection; text: string } | { op: "read"; selection: Selection };

/** The most a program may put on the clipboard at once. */
const MAX_BYTES = 1 << 20;

const STANDARD = /^[A-Za-z0-9+/]*={0,2}$/;

/** The text a base64 string stands for, or undefined when it is not base64 or not that small. */
export function decodeBase64(data: string): string | undefined {
  if (data.length % 4 !== 0 || !STANDARD.test(data) || (data.length / 4) * 3 > MAX_BYTES + 3) return undefined;
  try {
    const binary = atob(data);
    return new TextDecoder().decode(Uint8Array.from(binary, (c) => c.charCodeAt(0)));
  } catch {
    return undefined;
  }
}

export function encodeBase64(text: string): string {
  const bytes = new TextEncoder().encode(text);
  let binary = "";
  // In pieces: spreading a large array into one call overflows the stack.
  for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(binary);
}

/**
 * What an OSC 52 says: `Pc;Pd`, the clipboard it names (empty means the
 * standard one; Ghostty takes one letter and no more) and either `?` or the
 * text in base64. An empty write is a clear, which is left alone: the
 * person's clipboard is not a program's to wipe.
 */
export function parseOsc52(data: string): Osc52 | undefined {
  const semicolon = data.indexOf(";");
  if (semicolon < 0 || semicolon > 1) return undefined;
  const named = data.slice(0, semicolon);
  const selection: Selection = named === "s" || named === "p" ? named : "c";
  const payload = data.slice(semicolon + 1);
  if (payload === "?") return { op: "read", selection };
  if (payload === "") return undefined;
  const text = decodeBase64(payload);
  return text === undefined || text === "" ? undefined : { op: "write", selection, text };
}

/** What is sent back to a program that asked: the clipboard's text, or nothing when the answer is no. */
export function reply(selection: Selection, text: string): string {
  return `\x1b]52;${selection};${encodeBase64(text)}\x1b\\`;
}

/** The question asked before a program reads the clipboard. */
export function readQuestion(text: string): Question {
  return {
    title: "Permitir a leitura da área de transferência?",
    message: "Um programa neste terminal quer ler o que você copiou. É isto que ele receberia:",
    preview: preview(text),
    yes: "Permitir",
    no: "Negar",
  };
}

export interface ClipboardAccess {
  /** Put text on the system clipboard. */
  write(text: string): Promise<void>;
  /** The text on the system clipboard; empty when there is none. */
  read(): Promise<string>;
  /** Put a question to the person; true on the yes. */
  confirm(question: Question): Promise<boolean>;
  /** Send the program an answer, as if typed. */
  answer(data: string): void;
  /** A program put this text on the clipboard. */
  wrote?(text: string): void;
}

/** Act on one OSC 52, as Ghostty does: write at once, ask before reading. */
export async function handleOsc52(command: Osc52, access: ClipboardAccess): Promise<void> {
  if (command.op === "write") {
    if (command.selection !== "c") return;
    await access.write(command.text);
    access.wrote?.(command.text);
    return;
  }
  const text = await access.read().catch(() => "");
  // Nothing on it, nothing to give away: no question.
  const allowed = text === "" || (await access.confirm(readQuestion(text)));
  access.answer(reply(command.selection, allowed ? text : ""));
}

/** Let programs in the terminal write the clipboard, and read it when the person says so. */
export function installOsc52(term: Terminal, access: ClipboardAccess): IDisposable {
  return term.parser.registerOscHandler(52, (data) => {
    const command = parseOsc52(data);
    if (command) void handleOsc52(command, access).catch(() => {});
    return true;
  });
}
