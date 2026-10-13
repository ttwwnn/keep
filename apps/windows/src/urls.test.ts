// The addresses the terminal makes of its text: the macOS app's cases
// (tools/link-test/main.swift), including the one Claude Code broke across
// three rows.
import { describe, expect, it } from "vitest";
import { addressIn, addressesIn, filePath, hasScheme, isOpenable } from "./urls";

const url = (word: string) => addressIn(word)?.url;

describe("an address in a word", () => {
  it("is read to where the sentence takes over", () => {
    expect(url("https://example.com/a.")).toBe("https://example.com/a");
    expect(url("[docs](https://example.com/x)")).toBe("https://example.com/x");
    expect(url('"https://example.com/?q=1&b=2",')).toBe("https://example.com/?q=1&b=2");
    expect(url("(https://example.com)")).toBe("https://example.com");
    expect(url("https://example.com:")).toBe("https://example.com");
    expect(url("https://example.com/a?b=1#c,")).toBe("https://example.com/a?b=1#c");
  });

  it("keeps the brackets that belong to it", () => {
    expect(url("https://en.wikipedia.org/wiki/Foo_(bar)")).toBe("https://en.wikipedia.org/wiki/Foo_(bar)");
    expect(url("https://example.com/foo(bar)baz")).toBe("https://example.com/foo(bar)baz");
  });

  it("says where it sits in the word", () => {
    const hit = addressIn("[docs](https://example.com/x).")!;
    expect("[docs](https://example.com/x).".slice(hit.start, hit.end)).toBe("https://example.com/x");
  });

  it("makes a page of www, and takes mail and FTP", () => {
    expect(url("www.example.com.br/x")).toBe("https://www.example.com.br/x");
    expect(url("mailto:alguem@example.com")).toBe("mailto:alguem@example.com");
    expect(url("ftp://files.example.org/a")).toBe("ftp://files.example.org/a");
    expect(url("http://localhost:3000/x")).toBe("http://localhost:3000/x");
    expect(url("HTTPS://Example.com/A")).toBe("HTTPS://Example.com/A");
  });

  it("is not there in what only looks like one", () => {
    expect(url("Nota:")).toBeUndefined();
    expect(url("https://")).toBeUndefined();
    expect(url("http://foo")).toBeUndefined();
    expect(url("mailto:")).toBeUndefined();
    expect(url("profile:")).toBeUndefined();
    expect(url("ssh://example.com")).toBeUndefined();
    expect(url("vscode://file/tmp/a")).toBeUndefined();
    expect(url("palavra")).toBeUndefined();
    expect(url("http://[::1]:8000/")).toBeUndefined();
  });

  it("takes a file address for what it names, and not a word that ends in `file:`", () => {
    expect(url("file:///C:/Users/t/a%20b.txt")).toBe("file:///C:/Users/t/a%20b.txt");
    expect(url("(file:///tmp/a.txt),")).toBe("file:///tmp/a.txt");
    expect(url("file:")).toBeUndefined();
    expect(url("profile:/x")).toBeUndefined();
  });
});

describe("which a click may open", () => {
  it("is pages, mail, FTP and files; no app is started by its address", () => {
    for (const address of ["https://example.com", "HTTP://x.org", "mailto:a@b.c", "ftp://x.org/a", "file:///tmp/a.txt"]) {
      expect(isOpenable(address), address).toBe(true);
    }
    for (const address of ["vscode://file/tmp/a", "ms-msdt:/id", "javascript:alert(1)", "ssh://host", "tel:+55119", "C:\\x.exe", "/tmp/a", ""]) {
      expect(isOpenable(address), address).toBe(false);
    }
  });

  it("tells a scheme of more than one letter from a drive", () => {
    expect(hasScheme("https://x.com")).toBe(true);
    expect(hasScheme("mailto:x@y.z")).toBe(true);
    expect(hasScheme("C:\\x\\y")).toBe(false);
    expect(hasScheme("C:/x")).toBe(false);
    expect(hasScheme("a.txt")).toBe(false);
  });
});

// Claude Code fills a row to two columns short of the edge and goes on,
// indented, on the next.
const columns = 60;
const link =
  "https://github.com/ttwwnn/keep/blob/main/apps/macos/Sources/Keep/Model/LinkOpener.swift?plain=1&utm_source=keep&utm_medium=terminal&utm_campaign=links#L120";
const lead = "  o link é ";
const first = link.slice(0, columns - 2 - lead.length);
const second = link.slice(first.length, first.length + columns - 4);
const third = link.slice(first.length + second.length);
const broken = ["⏺ Pronto.", "", lead + first, "  " + second, "  " + third + " e também", "", "  Fim."];

/** A row is full when it reaches within a few columns of the edge. */
const fullOf = (lines: string[], width: number) => (line: number) => width > 0 && lines[line].replace(/\s+$/, "").length >= width - 3;

describe("an address Claude Code broke across rows", () => {
  it("is pieced back together from any of its three rows", () => {
    for (const row of [2, 3, 4]) {
      const hits = addressesIn(broken, row, fullOf(broken, columns));
      expect(hits.map((h) => h.url), `row ${row}`).toContain(link);
    }
  });

  it("says where each part is, and only the address's own", () => {
    const hit = addressesIn(broken, 3, fullOf(broken, columns)).find((h) => h.url === link)!;
    expect(hit.pieces.map((p) => broken[p.line].slice(p.start, p.end))).toEqual([first, second, third]);
    expect(hit.pieces.map((p) => p.line)).toEqual([2, 3, 4]);
  });

  it("keeps clear of the sentence below when the row is not full", () => {
    const lines = ["  veja https://example.com/a", "  Isso aqui"];
    expect(addressesIn(lines, 0, fullOf(lines, columns)).map((h) => h.url)).toEqual(["https://example.com/a"]);
  });

  it("is not joined to a full row before it that has nothing to do with it", () => {
    const lines = ["x".repeat(58), "https://example.com/a"];
    expect(addressesIn(lines, 1, fullOf(lines, columns)).map((h) => h.url)).toEqual(["https://example.com/a"]);
  });

  it("is found in no word of a row with none near", () => {
    expect(addressesIn(broken, 6, fullOf(broken, columns))).toEqual([]);
  });

  it("is not joined at all without the width", () => {
    expect(addressesIn(broken, 3, fullOf(broken, 0)).map((h) => h.url)).not.toContain(link);
  });

  it("finds two addresses on a row, each with its own place", () => {
    const line = "ver https://a.com e www.b.com.br, ok";
    const hits = addressesIn([line], 0, () => false);
    expect(hits.map((h) => h.url)).toEqual(["https://a.com", "https://www.b.com.br"]);
    expect(hits.map((h) => line.slice(h.pieces[0].start, h.pieces[0].end))).toEqual(["https://a.com", "www.b.com.br"]);
  });
});

describe("file addresses", () => {
  it("become the path of the platform", () => {
    expect(filePath("file:///C:/Users/t/a%20b.txt", "windows")).toBe("C:\\Users\\t\\a b.txt");
    expect(filePath("file://server/share/a.txt", "windows")).toBe("\\\\server\\share\\a.txt");
    expect(filePath("file:///tmp/a%20b.txt", "posix")).toBe("/tmp/a b.txt");
    expect(filePath("file:/tmp/a.txt", "posix")).toBe("/tmp/a.txt");
    expect(filePath("file://localhost/tmp/a.txt", "posix")).toBe("/tmp/a.txt");
  });

  it("are not paths where they cannot be", () => {
    expect(filePath("file:///tmp/a.txt", "windows")).toBeUndefined();
    expect(filePath("file://server/share/a.txt", "posix")).toBeUndefined();
    expect(filePath("file:", "posix")).toBeUndefined();
    expect(filePath("file:///a%zz", "posix")).toBeUndefined();
    expect(filePath("https://x.com", "posix")).toBeUndefined();
  });
});
