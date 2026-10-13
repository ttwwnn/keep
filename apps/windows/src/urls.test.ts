// The addresses the terminal underlines: Ghostty's cases (src/config/url.zig)
// for the schemes the app opens.
import { describe, expect, it } from "vitest";
import { filePath, findUrls, isAddress } from "./urls";

const urls = (text: string) => findUrls(text).map((m) => m.url);

describe("addresses in text", () => {
  it("finds an address among the words", () => {
    expect(urls("hello https://example.com world")).toEqual(["https://example.com"]);
    expect(urls("some file with https://google.com https://duckduckgo.com links.")).toEqual(["https://google.com", "https://duckduckgo.com"]);
    expect(urls("also match http://example.com non-secure links")).toEqual(["http://example.com"]);
    expect(urls("dot.http://example.com")).toEqual(["http://example.com"]);
  });

  it("keeps a query, a hash, a tilde and square brackets", () => {
    expect(urls("with query url https://example.com?query=1&other=2 and more text.")).toEqual(["https://example.com?query=1&other=2"]);
    expect(urls("weird https://example.com/~user/?query=1&other=2#hash and more")).toEqual(["https://example.com/~user/?query=1&other=2#hash"]);
    expect(urls("square brackets https://example.com/[foo] and more")).toEqual(["https://example.com/[foo]"]);
  });

  it("does not end in the sentence's punctuation or the wrapping's parenthesis", () => {
    expect(urls("Link period https://example.com. More text.")).toEqual(["https://example.com"]);
    expect(urls("Link trailing comma https://example.com, more text.")).toEqual(["https://example.com"]);
    expect(urls("Link inside (https://example.com) parens")).toEqual(["https://example.com"]);
    expect(urls('Link in double quotes "https://example.com" and more')).toEqual(["https://example.com"]);
    expect(urls("Link in single quotes 'https://example.com' and more")).toEqual(["https://example.com"]);
    expect(urls("see https://example.com: it is good")).toEqual(["https://example.com"]);
    expect(urls("really https://example.com?")).toEqual(["https://example.com"]);
  });

  it("keeps a parenthesis that belongs to the address", () => {
    expect(urls("https://example.com/foo(bar) more")).toEqual(["https://example.com/foo(bar)"]);
    expect(urls("https://example.com/foo(bar)baz more")).toEqual(["https://example.com/foo(bar)baz"]);
    expect(urls("https://en.wikipedia.org/wiki/Rust_(video_game) is a game")).toEqual(["https://en.wikipedia.org/wiki/Rust_(video_game)"]);
  });

  it("finds one in markdown and in brackets", () => {
    expect(urls("[mode 2027](https://github.com/contour-terminal/terminal-unicode-core) for better")).toEqual([
      "https://github.com/contour-terminal/terminal-unicode-core",
    ]);
    expect(urls("[13]:TooManyStatements: x [https://example.com/docs/Too-Many-Statements.md]")).toEqual([
      "https://example.com/docs/Too-Many-Statements.md",
    ]);
  });

  it("reads IPv6 hosts", () => {
    expect(urls("Serving HTTP on :: port 8000 (http://[::]:8000/)")).toEqual(["http://[::]:8000/"]);
    expect(urls("IPv6 address https://[2001:db8::1]:8080/path")).toEqual(["https://[2001:db8::1]:8080/path"]);
    expect(urls("IPv6 in markdown [link](http://[2001:db8::1]/docs)")).toEqual(["http://[2001:db8::1]/docs"]);
  });

  it("opens mail, phone and file addresses too", () => {
    expect(urls("mail mailto:test@example.com now")).toEqual(["mailto:test@example.com"]);
    expect(urls("match tel:+18005551234 tel links")).toEqual(["tel:+18005551234"]);
    expect(urls("match tel://+12123456789 phone numbers")).toEqual(["tel://+12123456789"]);
    expect(urls("see file:///C:/Users/t/a%20b.txt please")).toEqual(["file:///C:/Users/t/a%20b.txt"]);
  });

  it("leaves other schemes, bare schemes and plain words alone", () => {
    expect(urls("ssh://example.com ftp://x.org")).toEqual([]);
    expect(urls("https:// alone, and mailto: too")).toEqual([]);
    expect(urls("/tmp/test.txt http://www.google.com")).toEqual(["http://www.google.com"]);
  });

  it("says where each one sits", () => {
    const line = "ver https://a.com e https://b.com.";
    expect(findUrls(line).map((m) => line.slice(m.start, m.end))).toEqual(["https://a.com", "https://b.com"]);
  });

  it("goes through a very long address and a very long run without choking", () => {
    const long = "https://example.com/" + "a".repeat(5000) + ",";
    const started = Date.now();
    expect(findUrls(long)[0].url.length).toBe(long.length - 1);
    expect(findUrls("https://" + ".".repeat(5000)).length).toBe(0);
    // An IPv6 host that never closes.
    expect(findUrls("http://[" + ":a".repeat(3000) + "x").length).toBeLessThanOrEqual(1);
    expect(Date.now() - started).toBeLessThan(500);
  });
});

describe("addresses and drives", () => {
  it("tells a scheme of more than one letter from a drive", () => {
    expect(isAddress("https://x.com")).toBe(true);
    expect(isAddress("mailto:x@y.z")).toBe(true);
    expect(isAddress("C:\\x\\y")).toBe(false);
    expect(isAddress("C:/x")).toBe(false);
    expect(isAddress("a.txt")).toBe(false);
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
