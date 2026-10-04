// Small pieces of text handling with nothing to do with the window.

/**
 * Paths dropped on a terminal, as a shell takes them: one after another,
 * quoted when a space or a character the shells read specially would split
 * them. Double quotes are what PowerShell, cmd and bash all accept.
 */
export function quotePaths(paths: string[]): string {
  return paths
    .filter((p) => p.length > 0)
    .map((p) => (/[\s"'&()^%!;,`$]/.test(p) ? `"${p.replace(/"/g, '\\"')}"` : p))
    .join(" ");
}

/**
 * How well `query` fits `text`, for the picker: undefined when it does not,
 * otherwise a score where smaller is better. Every query letter must appear
 * in order; runs of consecutive letters and starts of words count in its
 * favour, as editors' "go to file" does.
 */
export function fuzzyScore(query: string, text: string): number | undefined {
  const q = query.trim().toLowerCase();
  if (q === "") return 0;
  const t = text.toLowerCase();
  // A plain substring always wins over a scattered match.
  const direct = t.indexOf(q);
  if (direct >= 0) return direct === 0 ? -1000 : -500 + direct;
  let score = 0;
  let at = 0;
  let last = -2;
  for (const c of q) {
    if (c === " ") continue;
    const found = t.indexOf(c, at);
    if (found < 0) return undefined;
    const startOfWord = found === 0 || /[\s/\\_\-.]/.test(t.charAt(found - 1));
    score += found - at + (found === last + 1 ? 0 : 3) - (startOfWord ? 2 : 0);
    last = found;
    at = found + 1;
  }
  return score;
}

/** Where `query` sits in `text`, for highlighting; empty when it does not. */
export function matchRanges(query: string, text: string): [number, number][] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  const t = text.toLowerCase();
  const direct = t.indexOf(q);
  if (direct >= 0) return [[direct, direct + q.length]];
  const out: [number, number][] = [];
  let at = 0;
  for (const c of q) {
    if (c === " ") continue;
    const found = t.indexOf(c, at);
    if (found < 0) return [];
    const prev = out[out.length - 1];
    if (prev && prev[1] === found) prev[1] = found + 1;
    else out.push([found, found + 1]);
    at = found + 1;
  }
  return out;
}

/** "há 5 min", "agora": how long ago, for a tooltip. */
export function ago(ms: number, now = Date.now()): string {
  if (!ms) return "";
  const seconds = Math.max(0, Math.round((now - ms) / 1000));
  if (seconds < 45) return "agora";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `há ${minutes} min`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `há ${hours} h`;
  const days = Math.round(hours / 24);
  return days === 1 ? "há 1 dia" : `há ${days} dias`;
}

/** A directory for a tooltip: home folded to `~`. */
export function homeRelative(path: string, home: string): string {
  if (!path) return "";
  if (home && path.toLowerCase().startsWith(home.toLowerCase())) {
    const rest = path.slice(home.length);
    return rest === "" ? "~" : `~${rest}`;
  }
  return path;
}
