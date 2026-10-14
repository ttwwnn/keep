// What a paste puts in the terminal: files first, as paths, else the text.
import { describe, expect, it } from "vitest";
import { pasteContent } from "./clipboardFiles";

describe("pasting with files on the clipboard", () => {
  it("types their paths, as a drop does, and leaves the text out", () => {
    expect(pasteContent(["C:\\Users\\t\\a.txt"], "a.txt", "other")).toBe("C:\\Users\\t\\a.txt ");
    expect(pasteContent(["C:\\Users\\t\\a b.txt", "C:\\x.png"], "", "other")).toBe('"C:\\Users\\t\\a b.txt" C:\\x.png ');
  });

  it("says them the way the pane's shell reads them", () => {
    expect(pasteContent(["C:\\Users\\t\\a b.txt"], "", "powershell")).toBe('"C:\\Users\\t\\a b.txt" ');
    expect(pasteContent(["C:\\Users\\t\\$x.txt"], "", "powershell")).toBe("'C:\\Users\\t\\$x.txt' ");
    expect(pasteContent(["C:\\Users\\t\\a b.txt"], "", "wsl")).toBe("/mnt/c/Users/t/a\\ b.txt ");
  });
});

describe("pasting without files", () => {
  it("is the text, untouched", () => {
    expect(pasteContent([], "ls -la\npwd", "other")).toBe("ls -la\npwd");
    expect(pasteContent([], "", "powershell")).toBe("");
  });

  it("ignores empty paths", () => {
    expect(pasteContent([""], "texto", "other")).toBe("texto");
  });
});
