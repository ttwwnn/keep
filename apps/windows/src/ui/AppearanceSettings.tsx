// The part of the settings that is the look: the appearance, the theme, the
// face and its size, the cursor and the history. The theme and the face are
// chosen from lists (ThemePicker, FontPicker), as in the macOS app's palette,
// and the chosen one is shown here as it will look.

import { useState } from "preact/hooks";
import { applyFont, clearTheme, resetText, setCursorBlink, setCursorShape, setScrollback } from "../appearance";
import { firstFamily, previewSample } from "../fonts";
import { SCROLLBACK_MAX, type CursorShape } from "../state";
import { fontFamily, gui, info, overlay, palette, setAppearance, setFont } from "../store";
import { isSingleTheme } from "../themes";
import { DEFAULT_FONT_SIZE, defaultFontFamily } from "../theme";
import { Swatch } from "./ThemePicker";

const SHAPES: [CursorShape, string][] = [
  ["bar", "Barra"],
  ["block", "Bloco"],
  ["underline", "Sublinhado"],
];

function Segments<T extends string | boolean>(props: {
  value: T;
  options: readonly (readonly [T, string])[];
  onPick(value: T): void;
  disabled?: boolean;
}) {
  return (
    <div class="segmented">
      {props.options.map(([value, label]) => (
        <button
          key={String(value)}
          class={props.value === value ? "on" : ""}
          disabled={props.disabled}
          onClick={() => props.onPick(value)}
        >
          {label}
        </button>
      ))}
    </div>
  );
}

export function AppearanceSettings() {
  const state = gui.value;
  const wearing = state.theme !== "";
  const pair = wearing && !isSingleTheme(state.theme);
  const terminal = palette.value.terminal;
  const [size, setSize] = useState(state.font.size || DEFAULT_FONT_SIZE);
  const [history, setHistory] = useState(String(state.terminal.scrollback));
  const face = state.font.family.trim();
  const platform = info.value?.platform ?? "";
  // The face as the terminal has it, for the name and the sample to be set in it.
  const shown = fontFamily.value;

  const keepHistory = () => {
    const lines = Number(history);
    if (history.trim() !== "" && Number.isFinite(lines)) setScrollback(lines);
    setHistory(String(gui.value.terminal.scrollback));
  };

  return (
    <>
      <fieldset>
        <legend>Aparência</legend>
        <Segments
          value={state.appearance}
          options={[
            ["auto", "Automático"],
            ["light", "Claro"],
            ["dark", "Escuro"],
          ]}
          onPick={setAppearance}
          disabled={wearing && !pair}
        />
        {wearing && !pair && <p class="hint">O tema escolhido define as cores; a aparência vale para o tema do Keep.</p>}
      </fieldset>
      <fieldset>
        <legend>Tema do terminal</legend>
        <div class="choice">
          <Swatch
            ground={terminal.background ?? "transparent"}
            dots={[terminal.red, terminal.green, terminal.yellow, terminal.blue, terminal.foreground].filter((c): c is string => !!c)}
          />
          <span class="choice-name">{wearing ? state.theme : "Keep (padrão)"}</span>
          <button onClick={() => (overlay.value = "themes")}>Escolher…</button>
          {wearing && <button onClick={clearTheme}>Usar o do Keep</button>}
        </div>
        <p class="hint">Muda também as cores da janela em volta do terminal.</p>
      </fieldset>
      <fieldset>
        <legend>Fonte</legend>
        <div class="choice">
          <span class="choice-name" style={{ fontFamily: shown }}>
            {face || `${firstFamily(defaultFontFamily(platform))} (padrão)`}
          </span>
          <button onClick={() => (overlay.value = "fonts")}>Escolher…</button>
          {face && <button onClick={() => applyFont("")}>Usar a padrão</button>}
        </div>
        <p class="face-sample sample-line" style={{ fontFamily: shown }}>
          {previewSample}
        </p>
        <div class="field-row">
          <span class="unit">Tamanho</span>
          <input
            class="number"
            type="number"
            min={6}
            max={48}
            step={0.25}
            value={size}
            onInput={(e) => {
              const n = Number((e.currentTarget as HTMLInputElement).value);
              setSize(n);
              if (n >= 6 && n <= 48) setFont(face, n);
            }}
          />
          <span class="unit">px</span>
          {(face || state.font.size !== 0 || state.zoom !== 1) && (
            <button
              class="small-button reset"
              onClick={() => {
                resetText();
                setSize(DEFAULT_FONT_SIZE);
              }}
            >
              Redefinir o texto
            </button>
          )}
        </div>
        <p class="hint">O zoom (Ctrl+= e Ctrl+-) multiplica este tamanho.</p>
      </fieldset>
      <fieldset>
        <legend>Cursor</legend>
        <div class="field-row cursor-row">
          <Segments value={state.terminal.cursor} options={SHAPES} onPick={setCursorShape} />
          <span class="unit">Piscar</span>
          <Segments
            value={state.terminal.blink}
            options={[
              [true, "Sim"],
              [false, "Não"],
            ]}
            onPick={setCursorBlink}
          />
        </div>
      </fieldset>
      <fieldset>
        <legend>Histórico</legend>
        <div class="field-row">
          <input
            class="number wide"
            type="number"
            min={0}
            max={SCROLLBACK_MAX}
            step={1000}
            value={history}
            onInput={(e) => setHistory((e.currentTarget as HTMLInputElement).value)}
            onBlur={keepHistory}
            onKeyDown={(e) => e.key === "Enter" && keepHistory()}
          />
          <span class="unit">linhas por aba</span>
        </div>
        <p class="hint">Cada 10 mil linhas podem usar uns 25 MB por aba.</p>
      </fieldset>
    </>
  );
}
