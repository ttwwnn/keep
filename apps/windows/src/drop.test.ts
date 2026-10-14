// What is typed for a file dropped on a terminal: the Mac's escape for a POSIX
// shell, and Windows' ways of saying a path to the shells there.
import { describe, expect, it } from "vitest";
import { dropText, escapePosix, shellKind, toWslPath } from "./drop";

describe("which shell a pane is at", () => {
  it("is told by the name of what runs in it", () => {
    expect(shellKind("pwsh")).toBe("powershell");
    expect(shellKind("PowerShell.exe")).toBe("powershell");
    expect(shellKind("wsl")).toBe("wsl");
    expect(shellKind("wsl.exe")).toBe("wsl");
    expect(shellKind("ubuntu2204")).toBe("wsl");
    expect(shellKind("claude")).toBe("other");
    expect(shellKind("cmd")).toBe("other");
    expect(shellKind("bash")).toBe("other");
    expect(shellKind("")).toBe("other");
  });
});

describe("a Windows path in WSL", () => {
  it("is a drive under /mnt, or a path inside a distribution", () => {
    expect(toWslPath("C:\\Users\\t\\a b.png")).toBe("/mnt/c/Users/t/a b.png");
    expect(toWslPath("D:\\")).toBe("/mnt/d");
    expect(toWslPath("d:/dados/x.csv")).toBe("/mnt/d/dados/x.csv");
    expect(toWslPath("\\\\wsl$\\Ubuntu\\home\\t\\x.txt")).toBe("/home/t/x.txt");
    expect(toWslPath("\\\\wsl.localhost\\Ubuntu\\home\\t")).toBe("/home/t");
    expect(toWslPath("\\\\wsl$\\Ubuntu")).toBe("/");
    expect(toWslPath("\\\\servidor\\pasta\\x")).toBeUndefined();
  });
});

describe("a path escaped as a POSIX shell wants it", () => {
  it("has a backslash before each character a shell takes as syntax, as the Mac does", () => {
    expect(escapePosix("/tmp/plain.png")).toBe("/tmp/plain.png");
    expect(escapePosix("/tmp/Captura de Tela (1).png")).toBe("/tmp/Captura\\ de\\ Tela\\ \\(1\\).png");
    expect(escapePosix("/tmp/a&b;c|d$e`f'g\"h*i?j#k!l^m")).toBe("/tmp/a\\&b\\;c\\|d\\$e\\`f\\'g\\\"h\\*i\\?j\\#k\\!l\\^m");
    expect(escapePosix("/tmp/á é ç.png")).toBe("/tmp/á\\ é\\ ç.png");
  });
});

describe("what is typed for the paths", () => {
  it("quotes a path with blanks and leaves a plain one, for cmd and the rest", () => {
    expect(dropText(["C:\\a\\b.txt"], "other")).toBe("C:\\a\\b.txt");
    expect(dropText(["C:\\Meus Documentos\\x.png", "C:\\y"], "other")).toBe('"C:\\Meus Documentos\\x.png" C:\\y');
    expect(dropText(["", "C:\\y"], "other")).toBe("C:\\y");
  });

  it("keeps a PowerShell string from expanding what a path has in it", () => {
    expect(dropText(["C:\\a b\\c.txt"], "powershell")).toBe('"C:\\a b\\c.txt"');
    expect(dropText(["C:\\Users\\t\\$Recycle.Bin\\x"], "powershell")).toBe("'C:\\Users\\t\\$Recycle.Bin\\x'");
    expect(dropText(["C:\\a\\it's.txt"], "powershell")).toBe("'C:\\a\\it''s.txt'");
    expect(dropText(["C:\\plain.txt"], "powershell")).toBe("C:\\plain.txt");
  });

  it("gives WSL the Linux path, escaped", () => {
    expect(dropText(["C:\\Users\\t\\Captura de Tela.png"], "wsl")).toBe("/mnt/c/Users/t/Captura\\ de\\ Tela.png");
    expect(dropText(["C:\\x\\a.txt", "D:\\y z\\b.txt"], "wsl")).toBe("/mnt/c/x/a.txt /mnt/d/y\\ z/b.txt");
    // A share WSL cannot reach is said the Windows way.
    expect(dropText(["\\\\srv\\share\\a b.txt"], "wsl")).toBe('"\\\\srv\\share\\a b.txt"');
  });
});
