// Which words of a terminal's text name a file or folder, and where they
// point. The cases are the macOS app's (tools/link-test/main.swift), with
// the file system standing in as a set of paths that exist.
import { describe, expect, it } from "vitest";
import {
  expand,
  flatten,
  looksLikePath,
  normalize,
  pathTokens,
  runsWhenOpened,
  suffixVariants,
  tokenAt,
  unescape,
  wordAt,
  type Where,
} from "./paths";

const posix: Where = { flavor: "posix", cwd: "/work", home: "/Users/t" };
const windows: Where = { flavor: "windows", cwd: "C:\\work", home: "C:\\Users\\t" };

/** What a click on `column` of `lines[row]` opens, and the text it covers on screen, given the paths that exist. */
function click(lines: string[], row: number, column: number, where: Where, exist: string[]) {
  const token = tokenAt(lines, row, column, where);
  if (!token) return undefined;
  const files = new Set(exist);
  for (const { path, candidate } of flatten(token)) {
    if (files.has(path)) {
      // Pieces of one row run together; a row to the next is told by a bar.
      const rows = new Map<number, [number, number]>();
      for (const p of candidate.pieces) {
        const seen = rows.get(p.line);
        rows.set(p.line, seen ? [Math.min(seen[0], p.start), Math.max(seen[1], p.end)] : [p.start, p.end]);
      }
      const covered = [...rows].map(([line, [from, to]]) => lines[line].slice(from, to)).join("|");
      return { path, covered };
    }
  }
  return undefined;
}

/** The same for a click on the first letter of `word` in the single line `line`. */
function opens(line: string, word: string, where: Where, exist: string[]) {
  return click([line], 0, line.indexOf(word), where, exist)?.path;
}

describe("spelling a printed path", () => {
  it("folds the dots and the separators the way the platform writes them", () => {
    expect(normalize("/a/b/../c/./d", "posix")).toBe("/a/c/d");
    expect(normalize("/../x", "posix")).toBe("/x");
    expect(normalize("C:/Users//t/./x/../y", "windows")).toBe("C:\\Users\\t\\y");
    expect(normalize("C:\\", "windows")).toBe("C:\\");
    expect(normalize("C:", "windows")).toBe("C:\\");
    expect(normalize("\\\\srv\\share\\a\\..\\b", "windows")).toBe("\\\\srv\\share\\b");
    expect(normalize("\\\\srv\\share", "windows")).toBe("\\\\srv\\share");
  });

  it("tries the path whole, then without the line and the column", () => {
    expect(suffixVariants("a.txt")).toEqual(["a.txt"]);
    expect(suffixVariants("a.txt:42")).toEqual(["a.txt:42", "a.txt"]);
    expect(suffixVariants("a.txt:42:7")).toEqual(["a.txt:42:7", "a.txt:42", "a.txt"]);
    expect(suffixVariants("a.txt:42:7:9")).toEqual(["a.txt:42:7:9", "a.txt:42:7", "a.txt:42"]);
    expect(suffixVariants("src/a.ts(12,5)")).toEqual(["src/a.ts(12,5)", "src/a.ts"]);
    expect(suffixVariants("C:\\x\\a.txt:3")).toEqual(["C:\\x\\a.txt:3", "C:\\x\\a.txt"]);
    expect(suffixVariants("C:\\x")).toEqual(["C:\\x"]);
  });

  it("resolves against the shell's directory, home and drives", () => {
    expect(expand("a.txt", posix)).toEqual(["/work/a.txt"]);
    expect(expand("../a.txt:4", { ...posix, cwd: "/work/sub dir" })).toEqual(["/work/a.txt:4", "/work/a.txt"]);
    expect(expand("~/x", posix)).toEqual(["/Users/t/x"]);
    expect(expand("~", posix)).toEqual(["/Users/t"]);
    expect(expand("a.txt", { ...posix, cwd: "" })).toEqual([]);
    expect(expand("~/x", { ...posix, home: "" })).toEqual([]);
    expect(expand("src\\a.ts:3:4", windows)).toEqual(["C:\\work\\src\\a.ts:3:4", "C:\\work\\src\\a.ts:3", "C:\\work\\src\\a.ts"]);
    expect(expand("~\\x", windows)).toEqual(["C:\\Users\\t\\x"]);
    expect(expand("..\\x", windows)).toEqual(["C:\\x"]);
    expect(expand("D:/data/x.csv", windows)).toEqual(["D:\\data\\x.csv"]);
    expect(expand("\\\\srv\\share\\x.txt", windows)).toEqual(["\\\\srv\\share\\x.txt"]);
    expect(expand("/mnt/c/Users/t/x.txt", windows)).toEqual(["C:\\Users\\t\\x.txt"]);
    expect(expand("/c/Users/t", windows)).toEqual(["C:\\Users\\t"]);
    expect(expand("/mnt/d", windows)).toEqual(["D:\\"]);
    // A POSIX root means nothing on Windows, and Windows' means nothing elsewhere.
    expect(expand("/usr/bin", windows)).toEqual([]);
    expect(expand("\\x", windows)).toEqual([]);
  });

  it("unescapes what a shell escaped", () => {
    expect(unescape("Captura\\ de\\ Tela.png", "posix")).toBe("Captura de Tela.png");
    expect(unescape("/a/b\\ \\(1\\).png", "windows")).toBe("/a/b (1).png");
    expect(unescape("C:\\a\\ b\\c.txt", "windows")).toBe("C:\\a b\\c.txt");
    expect(unescape("C:\\a\\b", "windows")).toBe("C:\\a\\b");
  });
});

describe("words", () => {
  it("reads the word under a cell, blanks escaped by a backslash staying inside it", () => {
    const line = "❯ olha /tmp/Captura\\ de\\ Tela.png agora";
    const [low, high] = wordAt(line, line.indexOf("de\\"), "posix")!;
    expect(line.slice(low, high)).toBe("/tmp/Captura\\ de\\ Tela.png");
    expect(wordAt(line, line.indexOf(" olha"), "posix")).toBeUndefined();
    expect(wordAt(line, 99, "posix")).toBeUndefined();
  });

  it("on Windows a backslash before a blank ends a folder unless the path is POSIX's", () => {
    const line = "cd C:\\x\\ && dir";
    const [low, high] = wordAt(line, line.indexOf("C:"), "windows")!;
    expect(line.slice(low, high)).toBe("C:\\x\\");
    const wsl = "ls /mnt/c/a\\ b/c.txt";
    const [l2, h2] = wordAt(wsl, wsl.indexOf("/mnt"), "windows")!;
    expect(wsl.slice(l2, h2)).toBe("/mnt/c/a\\ b/c.txt");
  });
});

describe("what looks like a path", () => {
  const yes = (word: string, where = posix) => expect(looksLikePath(word, where.flavor), word).toBe(true);
  const no = (word: string, where = posix) => expect(looksLikePath(word, where.flavor), word).toBe(false);

  it("is rooted, explicitly relative, or a folder and a file with a dot", () => {
    for (const word of ["/usr/bin/env", "~/x", "./a", "../a/b", "src/app.ts", "src/a/b", "lib/", "README.md", "package.json", "a.txt:42", "main.rs:3:9", "a/b.ts(12,5)", ".gitignore", "relatório.pdf"]) yes(word);
    for (const word of ["C:\\x\\y", "\\\\srv\\share\\a", "~\\x", ".\\a", "..\\a", "src\\app.ts", "node_modules\\.bin\\tsc", "C:/Users/t"]) yes(word, windows);
    for (const word of ["/mnt/c/Users/t", "/c/Users/t"]) yes(word, windows);
  });

  it("is not prose, a version, an address or a number", () => {
    for (const word of ["and/or", "24/7", "TCP/IP", "e.g.", "i", "v1.2.3", "3.14", "https://example.com/a/b.txt", "mailto:x@y.z", "word", "-", "//", "~", "."]) no(word);
    for (const word of ["C:\\", "and/or", "C:"]) no(word, { ...windows, flavor: "posix" });
    // POSIX roots that are not drives do not mean a path on Windows.
    for (const word of ["/usr/bin/env", "/help", "/c"]) no(word, windows);
  });
});

describe("a click on a path", () => {
  const root = "/tmp/keep-link";
  const exist = [
    `${root}`,
    `${root}/a.txt`,
    `${root}/sub dir`,
    `${root}/sub dir/b.md`,
    `${root}/odd:7`,
    "/Users/t",
  ];

  it("opens an absolute one, and one relative to the shell's directory", () => {
    const where: Where = { ...posix, cwd: root };
    expect(opens(`${root}/a.txt`, root, where, exist)).toBe(`${root}/a.txt`);
    expect(opens("a.txt", "a.txt", where, exist)).toBe(`${root}/a.txt`);
    expect(opens("a.txt", "a.txt", { ...where, cwd: "" }, exist)).toBeUndefined();
    expect(opens("../a.txt", "../a.txt", { ...where, cwd: `${root}/sub dir` }, exist)).toBe(`${root}/a.txt`);
  });

  it("leaves the :line and :line:col off, unless the name really ends in one", () => {
    const where: Where = { ...posix, cwd: root };
    expect(opens("a.txt:42", "a.txt", where, exist)).toBe(`${root}/a.txt`);
    expect(opens("a.txt:42:7", "a.txt", where, exist)).toBe(`${root}/a.txt`);
    expect(opens("./odd:7", "./odd", where, exist)).toBe(`${root}/odd:7`);
    expect(opens("nope.txt:3", "nope", where, exist)).toBeUndefined();
  });

  it("opens a folder, and home", () => {
    expect(opens(root, root, posix, exist)).toBe(root);
    expect(opens("ls ~/", "~/", posix, exist)).toBe("/Users/t");
  });

  it("reads through the escaped spaces of a dropped file", () => {
    const where: Where = { ...posix, cwd: root };
    expect(opens("sub\\ dir/b.md:3", "sub", where, exist)).toBe(`${root}/sub dir/b.md`);
    const shot = `${root}/Área de trabalho/Captura de Tela 2026-10-07 às 10.00.png`;
    const escaped = shot.replace(/ /g, "\\ ");
    const files = [shot];
    expect(opens(`❯ ${escaped} `, escaped.slice(0, 20), posix, files)).toBe(shot);
    // Clicked past the first escaped space.
    expect(opens(`❯ olha ${escaped}`, "trabalho/Captura", posix, files)).toBe(shot);
  });

  it("leaves the quotes, brackets and full stop around it", () => {
    const where: Where = { ...posix, cwd: root };
    const line = `Gravei em \`${root}/a.txt\`.`;
    const hit = click([line], 0, line.indexOf("/tmp"), where, exist)!;
    expect(hit.path).toBe(`${root}/a.txt`);
    expect(hit.covered).toBe(`${root}/a.txt`);
    expect(opens("(veja a.txt), depois", "a.txt", where, exist)).toBe(`${root}/a.txt`);
  });

  it("pieces a path back together that a program broke across two rows, from either row", () => {
    const deep = `${root}/projetos/clinica-integrativa/app/Filament/Resources/PacienteResource/Pages/EditPaciente.php`;
    const full = deep;
    const cut = full.length - 9;
    const head = full.slice(0, cut);
    const tail = full.slice(cut) + ":42";
    const lines = ["⏺ Abri o arquivo:", "", `  ${head}`, `  ${tail}`, "", "  Mais texto."];
    const first = click(lines, 2, 5, posix, [full]);
    expect(first?.path).toBe(full);
    expect(first?.covered).toBe(`${head}|${tail}`);
    expect(click(lines, 3, 5, posix, [full])?.path).toBe(full);
  });

  it("pieces a path over three rows", () => {
    const full = "/tmp/keep-link/projetos/clinica-integrativa/app/Filament/EditPaciente.php";
    const lines = ["  " + full.slice(0, 20), "  " + full.slice(20, 40), "  " + full.slice(40) + "."];
    expect(click(lines, 1, 6, posix, [full])?.path).toBe(full);
    expect(click(lines, 0, 6, posix, [full])?.path).toBe(full);
    expect(click(lines, 2, 6, posix, [full])?.path).toBe(full);
  });

  it("does not run a path on into a sentence on the next row", () => {
    const lines = [`  veja ${root}/a.txt`, "  e depois"];
    expect(click(lines, 0, 10, posix, [`${root}/a.txt`])?.path).toBe(`${root}/a.txt`);
    expect(click(lines, 0, 10, posix, [`${root}/a.txtdepois`])?.path).toBeUndefined();
  });

  it("finds nothing where nothing is", () => {
    expect(click([], 0, 0, posix, [])).toBeUndefined();
    expect(click(["  nope/x.txt", "  y"], 0, 4, { ...posix, cwd: root }, exist)).toBeUndefined();
    expect(click(["a b"], 0, 1, posix, exist)).toBeUndefined();
  });
});

describe("a click on a Windows path", () => {
  const exist = [
    "C:\\work",
    "C:\\work\\src\\app.ts",
    "C:\\work\\README.md",
    "C:\\Users\\t",
    "C:\\Users\\t\\Meus Documentos\\relatório final.pdf",
    "C:\\Program Files\\Keep\\keep.exe",
    "C:\\Program Files",
    "\\\\srv\\share\\x.txt",
    "D:\\data\\x.csv",
  ];

  it("opens drive, UNC, relative and home paths with either separator", () => {
    expect(opens("erro em C:\\Program Files\\Keep\\keep.exe agora", "C:\\", windows, exist)).toBe("C:\\Program Files\\Keep\\keep.exe");
    expect(opens("ver \\\\srv\\share\\x.txt", "\\\\srv", windows, exist)).toBe("\\\\srv\\share\\x.txt");
    expect(opens("D:/data/x.csv", "D:", windows, exist)).toBe("D:\\data\\x.csv");
    expect(opens("src\\app.ts:42:7: erro", "src", windows, exist)).toBe("C:\\work\\src\\app.ts");
    expect(opens("src/app.ts(12,5): error TS2304", "src", windows, exist)).toBe("C:\\work\\src\\app.ts");
    expect(opens(".\\src\\app.ts", ".\\src", windows, exist)).toBe("C:\\work\\src\\app.ts");
    expect(opens("README.md", "README", windows, exist)).toBe("C:\\work\\README.md");
    expect(opens("cd ~\\", "~", windows, exist)).toBe("C:\\Users\\t");
    expect(opens("PS C:\\work> ", "C:", windows, exist)).toBe("C:\\work");
  });

  it("follows a folder with a space in its name, as far as it goes", () => {
    const line = "ver C:\\Program Files\\Keep\\keep.exe --help";
    const hit = click([line], 0, line.indexOf("C:"), windows, exist)!;
    expect(hit.path).toBe("C:\\Program Files\\Keep\\keep.exe");
    expect(hit.covered).toBe("C:\\Program Files\\Keep\\keep.exe");
    // The shorter path that exists, when the longer does not.
    const folder = "abrir C:\\Program Files e depois";
    expect(click([folder], 0, folder.indexOf("C:"), windows, exist)?.path).toBe("C:\\Program Files");
    // It does not run into another path.
    const two = "cp C:\\work\\README.md C:\\Users\\t";
    expect(click([two], 0, two.indexOf("C:\\work"), windows, exist)?.path).toBe("C:\\work\\README.md");
    expect(click([two], 0, two.indexOf("C:\\Users"), windows, exist)?.path).toBe("C:\\Users\\t");
  });

  it("takes a quoted path whole, spaces and all", () => {
    const line = 'arrastei "C:\\Users\\t\\Meus Documentos\\relatório final.pdf" aqui';
    const hit = click([line], 0, line.indexOf("final"), windows, exist)!;
    expect(hit.path).toBe("C:\\Users\\t\\Meus Documentos\\relatório final.pdf");
    expect(hit.covered).toBe("C:\\Users\\t\\Meus Documentos\\relatório final.pdf");
  });

  it("reads WSL's drives", () => {
    expect(opens("/mnt/c/Users/t", "/mnt", windows, exist)).toBe("C:\\Users\\t");
  });
});

describe("tokens on a line", () => {
  const spans = (line: string, where = posix) => pathTokens([line], 0, where).map((t) => line.slice(t.at.start, t.at.end));

  it("picks out the path-like words and leaves the prose", () => {
    expect(spans("veja src/app.ts e/ou README.md, depois 24/7 v1.2.3 e.g. https://x.com/a/b.txt")).toEqual(["src/app.ts", "README.md,"]);
    expect(spans("(~/x/y.txt) and `/tmp/z`")).toEqual(["(~/x/y.txt)", "`/tmp/z`"]);
  });

  it("takes a quoted path as one token, inside its quotes", () => {
    const line = "abri 'meu dir/a b.txt' ok";
    expect(spans(line)).toContain("meu dir/a b.txt");
  });

  it("does not take the words between two apostrophes for a quoted path", () => {
    const line = "it's /tmp/x and don't";
    expect(spans(line)).toEqual(["/tmp/x"]);
  });

  it("skips a line too long to be worth the work", () => {
    expect(spans("/tmp/x ".repeat(1000))).toEqual([]);
  });
});

describe("opening", () => {
  it("does not run what Windows would run", () => {
    for (const file of ["C:\\x\\setup.exe", "a.BAT", "/a/run.py", "x.js", "link.lnk", "Makefile", ".env", "a.ps1", "C:\\x\\y.msi"]) {
      expect(runsWhenOpened(file), file).toBe(true);
    }
    for (const file of ["C:\\x\\notes.txt", "a.md", "/a/b.png", "C:\\x\\relatório.pdf", "c.tsx", "d.json", "e.docx", "f.rs"]) {
      expect(runsWhenOpened(file), file).toBe(false);
    }
  });
});
