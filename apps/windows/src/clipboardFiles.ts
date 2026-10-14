// What a paste puts in the terminal when the clipboard may hold files.
//
// The Explorer's Ctrl+C leaves the files on the clipboard and no text. Pasted
// in a terminal they arrive as their paths, typed as dropping them does
// (drop.ts) and said as the pane's shell reads them; the Mac app reads text
// only, so this is Windows' addition. The browser's paste event cannot say
// where the files are (it gives their contents), so the paths come from the
// app (`clipboard_files`, clipboard.rs) and the event only says that a paste
// happened.

import { dropText, type ShellKind } from "./drop";

/**
 * What to paste: the paths of the files on the clipboard, when there are any,
 * and otherwise the text. With files the text is not pasted at all, as nothing
 * in the Explorer's copy is text; the trailing blank is the one a drop leaves,
 * for whatever is typed next.
 */
export function pasteContent(paths: string[], text: string, shell: ShellKind): string {
  const typed = dropText(paths, shell);
  return typed === "" ? text : typed + " ";
}
