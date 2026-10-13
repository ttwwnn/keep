// Web addresses in the text of a terminal, and what a `file:` one stands for.
//
// The pattern is Ghostty's own (src/config/url.zig), the one the macOS app's
// terminal underlines under ⌘: liberal after the scheme, except that a URL
// does not end in a full stop or a comma, nor in a closing parenthesis unless
// it opened one (`…/Rust_(video_game)`; not `(https://example.com)`).
// Limited to the schemes the app opens: the opener plugin's scope covers
// http(s), mailto and tel, and `file:` is a path in the end.

import type { Flavor } from "./paths";

export interface UrlMatch {
  /** `[start, end)` in the text, in UTF-16 units. */
  start: number;
  end: number;
  url: string;
}

const SCHEMES = "https?://|mailto:|tel:|file:";
const CHARS = "[\\p{L}\\p{N}_\\-.~:/?#@!$&*+,;=%]";
const IPV6 = "\\[[:0-9a-fA-F]+\\](?::[0-9]+)?";
// One alternation under one `+`, so a long run of ordinary characters is
// matched in one pass and given back one by one, never split every way.
const URL = new RegExp(`(?:${SCHEMES})(?:${IPV6}|${CHARS}|[(\\[][\\p{L}\\p{N}_]*[)\\]])+(?<![,.])`, "giu");

/** The addresses in a line, in order. A trailing `:`, `;`, `!` or `?` belongs to the sentence. */
export function findUrls(text: string): UrlMatch[] {
  const out: UrlMatch[] = [];
  for (const match of text.matchAll(URL)) {
    let url = match[0];
    const trimmed = url.replace(/[:;!?]+$/, "");
    // Nothing left but the scheme: it was not an address.
    if (/^(?:https?:\/\/|mailto:|tel:|file:)$/i.test(trimmed)) continue;
    url = trimmed;
    out.push({ start: match.index, end: match.index + url.length, url });
  }
  return out;
}

/** A scheme of more than one letter makes an address; `C:\` is a drive. */
export function isAddress(text: string): boolean {
  return /^[A-Za-z][A-Za-z0-9+.-]+:/.test(text);
}

/** The file a `file:` address names, as a path of the platform, or undefined when it is not one. */
export function filePath(url: string, flavor: Flavor): string | undefined {
  const match = /^file:(?:\/\/([^/]*))?(\/.*)?$/i.exec(url);
  if (!match) return undefined;
  const host = match[1] ?? "";
  let path: string;
  try {
    path = decodeURIComponent(match[2] ?? "");
  } catch {
    return undefined;
  }
  if (flavor === "windows") {
    // file:///C:/Users/t/a.txt, and file://server/share/a.txt for a share.
    if (/^\/[A-Za-z]:/.test(path)) return path.slice(1).replace(/\//g, "\\");
    if (host !== "" && host !== "localhost") return `\\\\${host}${path.replace(/\//g, "\\")}`;
    return undefined;
  }
  if (host !== "" && host !== "localhost") return undefined;
  return path === "" ? undefined : path;
}
