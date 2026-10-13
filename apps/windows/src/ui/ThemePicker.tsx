// The themes, to choose one from: each row carries a scrap of the theme
// itself — its ground with its own colours on it — since the name of a theme
// says nothing (ThemeSwatchView in the macOS picker).

import { useEffect, useState } from "preact/hooks";
import { applyTheme, previewTheme } from "../appearance";
import { gui } from "../store";
import { isSingleTheme, listThemes, loadThemes, themeColors } from "../themes";
import { CatalogPicker, Marked } from "./CatalogPicker";

/** A scrap of a terminal: its ground, four of its colours that carry its character, and its letter. */
export function Swatch(props: { ground: string; dots: readonly string[] | null }) {
  return (
    <span class="swatch" style={{ background: props.ground }}>
      {props.dots?.map((colour, i) => <i key={i} style={{ background: colour }} />)}
    </span>
  );
}

export function ThemePicker() {
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    void loadThemes().catch(() => setFailed(true));
  }, []);
  const names = listThemes();
  const theme = gui.value.theme;
  return (
    <CatalogPicker
      label="Tema do terminal"
      placeholder="um tema para vestir…"
      names={names.length > 0 ? names : failed ? [] : null}
      none={failed ? "Não deu para ler os temas." : "Nenhum tema com esse nome."}
      worn={isSingleTheme(theme) ? theme : ""}
      row={(name, query, worn) => {
        const colors = themeColors(name);
        return (
          <>
            {/* A theme that would not read: an empty frame, which is honest, rather than somebody else's colours in its name. */}
            <Swatch
              ground={colors?.background ?? "transparent"}
              dots={colors ? [colors.ansi[1], colors.ansi[2], colors.ansi[3], colors.ansi[4], colors.foreground] : null}
            />
            <span class="catalog-name">
              <Marked name={name} query={query} />
            </span>
            {worn && <span class="catalog-note">em uso</span>}
          </>
        );
      }}
      onLook={previewTheme}
      onChoose={applyTheme}
    />
  );
}
