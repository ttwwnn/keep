// A program writing to the clipboard or asking for it: what is taken, what is
// asked first, and what is answered.
import { describe, expect, it } from "vitest";
import type { Terminal } from "@xterm/xterm";
import { decodeBase64, encodeBase64, handleOsc52, installOsc52, parseOsc52, reply, readQuestion, type ClipboardAccess } from "./osc52";

/** Base64 of the UTF-8 bytes, the long way, independent of the code under test. */
const b64 = (text: string) => {
  let binary = "";
  for (const byte of new TextEncoder().encode(text)) binary += String.fromCharCode(byte);
  return btoa(binary);
};

describe("OSC 52", () => {
  it("reads a write, and the clipboard it names", () => {
    expect(parseOsc52(`c;${b64("hello")}`)).toEqual({ op: "write", selection: "c", text: "hello" });
    expect(parseOsc52(`;${b64("hello")}`)).toEqual({ op: "write", selection: "c", text: "hello" });
    expect(parseOsc52(`s;${b64("hi")}`)).toEqual({ op: "write", selection: "s", text: "hi" });
    expect(parseOsc52(`p;${b64("hi")}`)).toEqual({ op: "write", selection: "p", text: "hi" });
    // Any other letter or digit is the standard clipboard.
    expect(parseOsc52(`0;${b64("hi")}`)).toEqual({ op: "write", selection: "c", text: "hi" });
    expect(parseOsc52(`q;${b64("hi")}`)).toEqual({ op: "write", selection: "c", text: "hi" });
    expect(parseOsc52(`c;${b64("ação 🇧🇷\nlinha 2")}`)).toEqual({ op: "write", selection: "c", text: "ação 🇧🇷\nlinha 2" });
  });

  it("reads a question", () => {
    expect(parseOsc52("c;?")).toEqual({ op: "read", selection: "c" });
    expect(parseOsc52(";?")).toEqual({ op: "read", selection: "c" });
    expect(parseOsc52("s;?")).toEqual({ op: "read", selection: "s" });
    expect(parseOsc52("p;?")).toEqual({ op: "read", selection: "p" });
    expect(parseOsc52("x;?")).toEqual({ op: "read", selection: "c" });
  });

  it("leaves out what is not one: no clipboard, more than one letter, a clear, bad base64, too much", () => {
    expect(parseOsc52("")).toBeUndefined();
    expect(parseOsc52("c")).toBeUndefined();
    expect(parseOsc52("cp;aGk=")).toBeUndefined();
    expect(parseOsc52("c;")).toBeUndefined();
    expect(parseOsc52(";")).toBeUndefined();
    expect(parseOsc52("c;aGk")).toBeUndefined();
    expect(parseOsc52("c;a!k=")).toBeUndefined();
    expect(parseOsc52("c;=aGk")).toBeUndefined();
    expect(parseOsc52(`c;${b64("x".repeat((1 << 20) + 64))}`)).toBeUndefined();
    expect(parseOsc52(`c;${b64("x".repeat(1 << 19))}`)?.op).toBe("write");
  });

  it("says what it answers the way Ghostty does", () => {
    expect(reply("c", "pasted")).toBe("\x1b]52;c;cGFzdGVk\x1b\\");
    expect(reply("c", "")).toBe("\x1b]52;c;\x1b\\");
    expect(reply("p", "ação")).toBe(`\x1b]52;p;${b64("ação")}\x1b\\`);
    expect(decodeBase64(encodeBase64("x".repeat(100000)))).toBe("x".repeat(100000));
  });

  it("asks with the text it would hand over", () => {
    const question = readQuestion("segredo");
    expect(question.preview).toBe("segredo");
    expect([question.yes, question.no]).toEqual(["Permitir", "Negar"]);
  });
});

function access(clipboard: string, allow: boolean) {
  const log: string[] = [];
  const impl: ClipboardAccess = {
    write: async (text) => void log.push(`write:${text}`),
    read: async () => clipboard,
    confirm: async () => (log.push("asked"), allow),
    answer: (data) => void log.push(`answer:${data}`),
    wrote: (text) => void log.push(`wrote:${text}`),
  };
  return { impl, log };
}

describe("what is done with it", () => {
  it("writes the standard clipboard at once, and says it was a program that wrote", async () => {
    const { impl, log } = access("", false);
    await handleOsc52({ op: "write", selection: "c", text: "copiado" }, impl);
    expect(log).toEqual(["write:copiado", "wrote:copiado"]);
  });

  it("drops a write to a selection the machine has none of", async () => {
    const { impl, log } = access("", false);
    await handleOsc52({ op: "write", selection: "p", text: "x" }, impl);
    await handleOsc52({ op: "write", selection: "s", text: "x" }, impl);
    expect(log).toEqual([]);
  });

  it("asks before it reads, and answers what the person said", async () => {
    const yes = access("conteúdo", true);
    await handleOsc52({ op: "read", selection: "c" }, yes.impl);
    expect(yes.log).toEqual(["asked", `answer:${reply("c", "conteúdo")}`]);
    const no = access("conteúdo", false);
    await handleOsc52({ op: "read", selection: "c" }, no.impl);
    expect(no.log).toEqual(["asked", "answer:\x1b]52;c;\x1b\\"]);
  });

  it("answers a read of a selection with the clipboard there is, under the name asked", async () => {
    const { impl, log } = access("x", true);
    await handleOsc52({ op: "read", selection: "s" }, impl);
    expect(log[1]).toBe(`answer:${reply("s", "x")}`);
  });

  it("does not ask what has nothing to give away", async () => {
    const { impl, log } = access("", false);
    await handleOsc52({ op: "read", selection: "c" }, impl);
    expect(log).toEqual(["answer:\x1b]52;c;\x1b\\"]);
  });

  it("answers nothing, rather than failing, when the clipboard cannot be read", async () => {
    const { impl, log } = access("x", true);
    impl.read = async () => {
      throw new Error("sem acesso");
    };
    await handleOsc52({ op: "read", selection: "c" }, impl);
    expect(log).toEqual(["answer:\x1b]52;c;\x1b\\"]);
  });
});

describe("the terminal's side", () => {
  it("takes OSC 52 as its own, whatever it says", async () => {
    let handler: ((data: string) => boolean | Promise<boolean>) | undefined;
    let number = 0;
    const term = {
      parser: {
        registerOscHandler: (ident: number, callback: (data: string) => boolean) => {
          number = ident;
          handler = callback;
          return { dispose: () => (handler = undefined) };
        },
      },
    } as unknown as Terminal;
    const { impl, log } = access("", false);
    const installed = installOsc52(term, impl);
    expect(number).toBe(52);
    expect(handler?.(`c;${b64("oi")}`)).toBe(true);
    expect(handler?.("lixo")).toBe(true);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(log).toEqual(["write:oi", "wrote:oi"]);
    installed.dispose();
    expect(handler).toBeUndefined();
  });
});
