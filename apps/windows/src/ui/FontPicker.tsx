// The monospaced faces installed here, to choose one from: each row is set in
// itself, with the glyphs a terminal is judged on beside it (fontCell in the
// macOS picker), since the name of a face tells almost nothing about its zero.

import { useEffect, useState } from "preact/hooks";
import { applyFont, previewFont } from "../appearance";
import { listMonospaceFonts, previewSample, quoteFamily } from "../fonts";
import { gui } from "../store";
import { CatalogPicker, Marked } from "./CatalogPicker";

export function FontPicker() {
  const [names, setNames] = useState<string[] | null>(null);
  useEffect(() => {
    void listMonospaceFonts().then(setNames, () => setNames([]));
  }, []);
  return (
    <CatalogPicker
      label="Fonte do terminal"
      placeholder="uma fonte para o terminal…"
      names={names}
      none={names === null ? "Procurando…" : "Nenhuma fonte monoespaçada com esse nome."}
      worn={gui.value.font.family}
      row={(name, query, worn) => {
        const face = { fontFamily: `${quoteFamily(name)}, monospace` };
        return (
          <>
            <span class="catalog-name face" style={face}>
              <Marked name={name} query={query} />
            </span>
            <span class="face-sample" style={face}>
              {previewSample}
            </span>
            {worn && <span class="catalog-note">em uso</span>}
          </>
        );
      }}
      onLook={previewFont}
      onChoose={applyFont}
    />
  );
}
