#!/usr/bin/env node
// Builds the theme catalogue (src/themes/catalog.json) from the Ghostty-format
// themes of https://github.com/mbadolato/iTerm2-Color-Schemes (MIT; the
// licence and the credits are copied beside it).
//
//   node tools/gerar-temas.mjs                  # clones the repository (its ghostty folder only)
//   node tools/gerar-temas.mjs --from <clone>   # builds from a clone already at hand
//
// Needs Node 22.18 or later, which runs the TypeScript module the app reads
// the catalogue with. The output is sorted and names the commit it was made
// from, so the same commit gives the same file.

import { execFileSync } from "node:child_process";
import { copyFileSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { packTheme, parseGhosttyTheme } from "../src/themes/ghostty.ts";

const REPOSITORY = "https://github.com/mbadolato/iTerm2-Color-Schemes";
const here = dirname(fileURLToPath(import.meta.url));
const out = resolve(here, "../src/themes");

const fromAt = process.argv.indexOf("--from");
let clone = fromAt >= 0 ? resolve(process.argv[fromAt + 1] ?? "") : "";
let temporary = "";
if (!clone) {
  temporary = mkdtempSync(join(tmpdir(), "keep-temas-"));
  clone = join(temporary, "schemes");
  const git = (...args) => execFileSync("git", args, { stdio: ["ignore", "inherit", "inherit"] });
  git("clone", "--depth", "1", "--filter=blob:none", "--sparse", REPOSITORY, clone);
  git("-C", clone, "sparse-checkout", "set", "ghostty");
}

try {
  const commit = execFileSync("git", ["-C", clone, "rev-parse", "HEAD"], { encoding: "utf8" }).trim();
  const folder = join(clone, "ghostty");
  const collator = new Intl.Collator("en", { sensitivity: "base" });
  const names = readdirSync(folder).filter((name) => !name.startsWith(".")).sort((a, b) => collator.compare(a, b) || (a < b ? -1 : 1));

  const themes = [];
  const rejected = [];
  for (const name of names) {
    const theme = parseGhosttyTheme(readFileSync(join(folder, name), "utf8"));
    if (theme) themes.push([name, packTheme(theme)]);
    else rejected.push(name);
  }
  if (rejected.length > 0) console.error(`temas recusados (cores de menos): ${rejected.join(", ")}`);
  if (themes.length === 0) throw new Error(`nenhum tema em ${folder}`);

  // One theme to a line, so a change in the catalogue reads as one in a diff.
  const lines = themes.map((entry) => `    ${JSON.stringify(entry)}`).join(",\n");
  writeFileSync(
    join(out, "catalog.json"),
    `{\n  "source": ${JSON.stringify(REPOSITORY)},\n  "commit": ${JSON.stringify(commit)},\n  "themes": [\n${lines}\n  ]\n}\n`,
  );
  copyFileSync(join(clone, "LICENSE"), join(out, "LICENSE-iterm2-color-schemes"));
  copyFileSync(join(clone, "CREDITS.md"), join(out, "CREDITS-iterm2-color-schemes.md"));
  console.log(`${themes.length} temas de ${REPOSITORY} @ ${commit.slice(0, 12)} em ${out}`);
} finally {
  if (temporary) rmSync(temporary, { recursive: true, force: true });
}
