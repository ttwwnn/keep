// What may be pasted into a terminal without asking.
//
// The macOS app's terminal (Ghostty) asks first when a paste would run itself:
// text with a line break, pasted where the program did not ask for bracketed
// paste, is read by it as Enter at every break (Surface.zig
// `completeClipboardPaste`). And whatever is pasted loses the control
// characters a tty acts on — an Escape, a Ctrl+C, a Ctrl+U — which xterm
// does too (`paste.zig` `encode`); xterm.js does neither.

/** A question put to the person, with what it is about shown as it is. */
export interface Question {
  title: string;
  message: string;
  /** The text the question is about, shown in a box. */
  preview?: string;
  yes: string;
  no: string;
}

/** The end of a bracketed paste: in the text, it lets the rest of it out of the frame. */
const END_OF_PASTE = "\x1b[201~";

/** How much of the text the question shows. */
export const PREVIEW_CHARS = 400;

/**
 * Whether pasting would not run the text: bracketed paste frames it, so a
 * line break is only text, unless the text itself carries the end of the
 * frame; unframed, a line break is Enter. A lone carriage return is Enter too
 * (the Mac rule counts only line feeds; a Windows clipboard always has both).
 */
export function isSafePaste(text: string, bracketed: boolean): boolean {
  if (text.includes(END_OF_PASTE)) return false;
  return bracketed || !/[\r\n]/.test(text);
}

/** The control characters a tty acts on, which a paste takes out: xterm's own list (NUL, ^C, ^D, ^E, ^H, ^O, ^Q–^S, ^U–^W, ^Z, ESC, ^\, DEL). */
const STRIPPED = /[\x00\x03-\x05\x08\x0f\x11-\x13\x15-\x17\x1a-\x1c\x7f]/g;

/** The text as it is pasted: those characters become spaces. */
export function preparePaste(text: string): string {
  return text.replace(STRIPPED, " ");
}

/** The first characters of a text, to show it without filling the window. */
export function preview(text: string): string {
  const chars = Array.from(text);
  return chars.length > PREVIEW_CHARS ? chars.slice(0, PREVIEW_CHARS).join("") + "…" : text;
}

/** The question asked before a paste that would run itself. */
export function pasteQuestion(text: string): Question {
  const framed = text.includes(END_OF_PASTE);
  return {
    title: "Colar isto?",
    message: framed
      ? "O texto contém a sequência que encerra a colagem protegida do terminal: o que vem depois dela seria executado como se fosse digitado."
      : "O texto tem quebras de linha, e o programa em execução aqui lê cada uma como Enter: ele será executado assim que for colado.",
    preview: preview(text),
    yes: "Colar",
    no: "Cancelar",
  };
}
