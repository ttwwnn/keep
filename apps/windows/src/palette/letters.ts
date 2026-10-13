// Matching without the accents: a person typing `configuracoes` means
// `Configurações`, and `acao` means `ação`. The macOS picker's titles are in
// English and never needed this.

/**
 * `text` with each accented letter as its plain one, and nothing else changed —
 * the same length, unit for unit, so a range found in this is a range of the
 * text it came from. A letter that is written as a base and a combining mark
 * (what a file name on a Mac holds) is left as two: the mark stays, where it
 * matches nothing a person types.
 */
export function plainLetters(text: string): string {
  let out = "";
  for (const char of text) {
    const parts = char.normalize("NFD");
    const mark = parts.length > 1 ? parts.charCodeAt(1) : 0;
    out += mark >= 0x300 && mark <= 0x36f ? parts[0] : char;
  }
  return out;
}
