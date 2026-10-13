// The flags a program turns on, and the answer to its question: the stack of
// each screen, and the sequences that move it.
import { describe, expect, it } from "vitest";
import type { Terminal } from "@xterm/xterm";
import { FlagStack, installKitty } from "./kitty";

describe("the flags of a screen", () => {
  it("start off, and a push puts new ones on and a pop brings the old ones back", () => {
    const stack = new FlagStack();
    expect(stack.current).toBe(0);
    stack.push(1);
    stack.push(5);
    expect(stack.current).toBe(5);
    stack.pop(1);
    expect(stack.current).toBe(1);
    stack.pop(1);
    expect(stack.current).toBe(0);
  });

  it("are set, added to and taken from", () => {
    const stack = new FlagStack();
    stack.set(1, 0b00101);
    expect(stack.current).toBe(0b00101);
    stack.set(2, 0b01000);
    expect(stack.current).toBe(0b01101);
    stack.set(3, 0b00100);
    expect(stack.current).toBe(0b01001);
    stack.set(1, 0b00010);
    expect(stack.current).toBe(0b00010);
  });

  it("forget the oldest when eight pushes are past", () => {
    const stack = new FlagStack();
    for (let flags = 1; flags <= 9; flags++) stack.push(flags);
    expect(stack.current).toBe(9);
    // Seven pops reach back to the second push; the first has been written over.
    stack.pop(7);
    expect(stack.current).toBe(2);
    stack.pop(1);
    expect(stack.current).toBe(0);
  });

  it("are emptied by a pop of as many as there can be", () => {
    const stack = new FlagStack();
    stack.push(1);
    stack.push(2);
    stack.pop(8);
    expect(stack.current).toBe(0);
    stack.push(3);
    stack.pop(1000);
    expect(stack.current).toBe(0);
  });

  it("are not made negative by a pop past the bottom", () => {
    const stack = new FlagStack();
    stack.pop(3);
    expect(stack.current).toBe(0);
    stack.push(4);
    expect(stack.current).toBe(4);
  });
});

type Handler = (params: (number | number[])[]) => boolean;

/** A terminal as far as the protocol reads it: the parser's handlers, and the screen it is on. */
function fake() {
  const csi = new Map<string, Handler>();
  let esc: (() => boolean) | undefined;
  let onChange: (() => void) | undefined;
  const state = { alternate: false, disposed: 0 };
  const term = {
    parser: {
      registerCsiHandler: (id: { prefix?: string; final: string }, callback: Handler) => {
        csi.set(`${id.prefix ?? ""}${id.final}`, callback);
        return { dispose: () => void state.disposed++ };
      },
      registerEscHandler: (id: { final: string }, callback: () => boolean) => {
        expect(id.final).toBe("c");
        esc = callback;
        return { dispose: () => void state.disposed++ };
      },
    },
    buffer: {
      get active() {
        return { type: state.alternate ? "alternate" : "normal" };
      },
      onBufferChange: (callback: () => void) => {
        onChange = callback;
        return { dispose: () => void state.disposed++ };
      },
    },
  } as unknown as Terminal;
  const said: string[] = [];
  const kitty = installKitty(term, (data) => said.push(data));
  const send = (sequence: string, ...params: (number | number[])[]) => {
    const handler = csi.get(sequence);
    expect(handler, sequence).toBeDefined();
    expect(handler!(params), sequence).toBe(true);
  };
  const enterAlternate = () => {
    state.alternate = true;
    onChange?.();
  };
  const leaveAlternate = () => {
    state.alternate = false;
    onChange?.();
  };
  return { kitty, said, send, enterAlternate, leaveAlternate, reset: () => esc?.(), state, csi };
}

describe("a program turning the protocol on", () => {
  it("is off until it pushes, and is told so when it asks", () => {
    const t = fake();
    expect(t.kitty.flags()).toBe(0);
    t.send("?u", 0);
    expect(t.said).toEqual(["\x1b[?0u"]);
  });

  it("pushes flags, asks what is on, and pops them on its way out", () => {
    const t = fake();
    t.send(">u", 5);
    expect(t.kitty.flags()).toBe(5);
    t.send("?u", 0);
    expect(t.said).toEqual(["\x1b[?5u"]);
    t.send(">u", 1);
    expect(t.kitty.flags()).toBe(1);
    t.send("<u", 0);
    expect(t.kitty.flags()).toBe(5);
    t.send("<u", 1);
    expect(t.kitty.flags()).toBe(0);
  });

  it("pops as many as it says", () => {
    const t = fake();
    t.send(">u", 1);
    t.send(">u", 2);
    t.send(">u", 3);
    t.send("<u", 2);
    expect(t.kitty.flags()).toBe(1);
  });

  it("sets the flags, adds to them, or takes from them", () => {
    const t = fake();
    t.send("=u", 5, 1);
    expect(t.kitty.flags()).toBe(5);
    t.send("=u", 2, 2);
    expect(t.kitty.flags()).toBe(7);
    t.send("=u", 1, 3);
    expect(t.kitty.flags()).toBe(6);
    // The repaint of the daemon says the flags in this way, a mode of 1 and no more.
    t.send("=u", 1, 1);
    expect(t.kitty.flags()).toBe(1);
    t.send("=u", 4);
    expect(t.kitty.flags()).toBe(4);
  });

  it("is not heeded for flags there are not, or a mode there is not", () => {
    const t = fake();
    t.send(">u", 5);
    t.send(">u", 32);
    expect(t.kitty.flags()).toBe(5);
    t.send("=u", 99, 1);
    t.send("=u", 1, 4);
    t.send("=u", 1, 0);
    expect(t.kitty.flags()).toBe(5);
  });

  it("takes the first of a list of sub-parameters", () => {
    const t = fake();
    t.send(">u", [5, 9]);
    expect(t.kitty.flags()).toBe(5);
  });

  it("has a screen of its own on each of the two, and the alternate starts new each time", () => {
    const t = fake();
    t.send(">u", 1);
    t.enterAlternate();
    expect(t.kitty.flags()).toBe(0);
    t.send("?u", 0);
    expect(t.said).toEqual(["\x1b[?0u"]);
    t.send(">u", 31);
    expect(t.kitty.flags()).toBe(31);
    t.leaveAlternate();
    expect(t.kitty.flags()).toBe(1);
    t.enterAlternate();
    // The program that left without popping is not heard from again.
    expect(t.kitty.flags()).toBe(0);
    t.leaveAlternate();
    expect(t.kitty.flags()).toBe(1);
  });

  it("is turned off by a full reset on both screens, and the terminal's reset goes on", () => {
    const t = fake();
    t.send(">u", 5);
    t.enterAlternate();
    t.send(">u", 5);
    expect(t.reset()).toBe(false);
    expect(t.kitty.flags()).toBe(0);
    t.leaveAlternate();
    expect(t.kitty.flags()).toBe(0);
  });

  it("lets go of the terminal's handlers", () => {
    const t = fake();
    t.kitty.dispose();
    expect(t.state.disposed).toBe(6);
  });

  it("takes the four sequences, and only those", () => {
    const t = fake();
    expect([...t.csi.keys()].sort()).toEqual(["<u", "=u", ">u", "?u"]);
  });
});
