// What floats over the terminal: the picker (ui/Picker.tsx: Ir para, the
// palette, the searches), the settings, and the questions the window asks
// before it ends something.
//
// Nothing is painted over the terminal behind a card (DESIGN.md): the card's
// shadow says it is above, and says it where the card is.

import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useEffect, useRef, useState } from "preact/hooks";
import * as api from "../api";
import { activeView, answer, dialog, gui, info, overlay, setAppearance, setFont, type DialogField } from "../store";
import { defaultFontFamily, DEFAULT_FONT_SIZE } from "../theme";
import { Picker } from "./Picker";

function close() {
  overlay.value = null;
  requestAnimationFrame(() => activeView()?.focus());
}

/** Focus a field once it is in the page: `autofocus` only works on load. */
function useFocus<T extends HTMLElement>(when = true) {
  const ref = useRef<T>(null);
  useEffect(() => {
    if (!when) return;
    ref.current?.focus();
    if (ref.current instanceof HTMLInputElement) ref.current.select();
  }, [when]);
  return ref;
}

function Settings() {
  const state = gui.value;
  const [shells, setShells] = useState<{ chosen: string; available: api.Shell[] } | null>(null);
  const [family, setFamily] = useState(state.font.family);
  const [size, setSize] = useState(state.font.size || DEFAULT_FONT_SIZE);
  useEffect(() => {
    void api.shells().then(setShells).catch(() => setShells({ chosen: "", available: [] }));
  }, []);
  const platform = info.value?.platform ?? "";
  return (
    <div class="card settings" role="dialog" aria-label="Configurações" onKeyDown={(e) => e.key === "Escape" && close()}>
      <h2>Configurações</h2>
      <fieldset>
        <legend>Aparência</legend>
        <div class="segmented">
          {(
            [
              ["auto", "Automático"],
              ["light", "Claro"],
              ["dark", "Escuro"],
            ] as const
          ).map(([value, label]) => (
            <button key={value} class={state.appearance === value ? "on" : ""} onClick={() => setAppearance(value)}>
              {label}
            </button>
          ))}
        </div>
      </fieldset>
      <fieldset>
        <legend>Fonte</legend>
        <div class="field-row">
          <input
            value={family}
            placeholder={defaultFontFamily(platform)}
            spellcheck={false}
            onInput={(e) => setFamily((e.currentTarget as HTMLInputElement).value)}
            onBlur={() => setFont(family, size)}
            onKeyDown={(e) => e.key === "Enter" && setFont(family, size)}
          />
          <input
            class="number"
            type="number"
            min={6}
            max={48}
            value={size}
            onInput={(e) => {
              const n = Number((e.currentTarget as HTMLInputElement).value);
              setSize(n);
              if (n >= 6 && n <= 48) setFont(family, n);
            }}
          />
          <span class="unit">px</span>
        </div>
        <p class="hint">O zoom (Ctrl+= e Ctrl+-) multiplica este tamanho.</p>
      </fieldset>
      <fieldset>
        <legend>Shell das abas novas</legend>
        {shells === null ? (
          <p class="hint">Procurando…</p>
        ) : (
          <select
            value={shells.chosen}
            onChange={(e) => {
              const command = (e.currentTarget as HTMLSelectElement).value;
              void api.chooseShell(command).then(() => setShells({ ...shells, chosen: command }));
            }}
          >
            <option value="">Padrão (o melhor instalado)</option>
            {shells.available.map((s) => (
              <option key={s.id} value={s.command}>
                {s.label}
              </option>
            ))}
            {shells.chosen && !shells.available.some((s) => s.command === shells.chosen) && (
              <option value={shells.chosen}>{shells.chosen}</option>
            )}
          </select>
        )}
        <p class="hint">Vale para as próximas abas; as abertas seguem com o seu.</p>
      </fieldset>
      <p class="about">
        Keep {info.value?.version} · <span class="mono">{info.value?.address}</span>
      </p>
      <div class="card-buttons">
        <button class="primary" onClick={close}>
          Pronto
        </button>
      </div>
    </div>
  );
}

function FieldInput(props: { field: DialogField; values: Record<string, string>; set: (name: string, value: string) => void; first: boolean; submit: () => void }) {
  const { field, values, set, first, submit } = props;
  const input = useFocus<HTMLInputElement>(first);
  return (
    <label class="dialog-field">
      <span>{field.label}</span>
      <div class="field-row">
        <input
          ref={input}
          value={values[field.name] ?? ""}
          placeholder={field.placeholder}
          spellcheck={false}
          onInput={(e) => set(field.name, (e.currentTarget as HTMLInputElement).value)}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === "Enter") submit();
            if (e.key === "Escape") answer(null);
          }}
        />
        {field.folder && (
          <button
            type="button"
            onClick={async () => {
              const picked = await openDialog({ directory: true, multiple: false, title: field.label }).catch(() => null);
              if (typeof picked === "string") set(field.name, picked);
            }}
          >
            Escolher…
          </button>
        )}
      </div>
    </label>
  );
}

function DialogView() {
  const current = dialog.value;
  const [values, setValues] = useState<Record<string, string>>({});
  const primary = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!current) return;
    setValues(Object.fromEntries((current.fields ?? []).map((f) => [f.name, f.value])));
    if (!current.fields?.length) queueMicrotask(() => primary.current?.focus());
  }, [current]);
  if (!current) return null;
  const main = current.buttons.find((b) => b.primary) ?? current.buttons[current.buttons.length - 1];
  const submit = () => answer(main.value, values);
  return (
    <div class="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && answer(null)}>
      <div
        class="card dialog"
        role="alertdialog"
        aria-modal="true"
        aria-label={current.title}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === "Escape") answer(null);
        }}
      >
        <h2>{current.title}</h2>
        {current.message && <p>{current.message}</p>}
        {current.fields?.map((field, i) => (
          <FieldInput
            key={field.name}
            field={field}
            values={values}
            first={i === 0}
            submit={submit}
            set={(name, value) => setValues((v) => ({ ...v, [name]: value }))}
          />
        ))}
        <div class="card-buttons">
          {current.buttons.map((b) => (
            <button
              key={b.value}
              ref={b === main ? primary : undefined}
              class={`${b.primary ? "primary" : ""} ${b.danger ? "danger" : ""}`}
              onClick={() => answer(b.value, values)}
            >
              {b.label}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

export function Overlays() {
  const which = overlay.value;
  useEffect(() => {
    if (!which) return;
    // A click outside the card closes it, as a menu does.
    const onDown = (e: MouseEvent) => {
      const target = e.target as HTMLElement;
      // The button that opened it closes it itself, on its click; and a question asked over it is answered, not clicked away.
      if (!target.closest(".card") && !target.closest("[data-overlay-toggle]") && !target.closest(".modal-backdrop")) close();
    };
    window.addEventListener("mousedown", onDown);
    return () => window.removeEventListener("mousedown", onDown);
  }, [which]);
  return (
    <>
      {(which === "picker" || which === "palette" || which === "find" || which === "search") && <Picker key={which} mode={which} />}
      {which === "settings" && <Settings />}
      <DialogView />
    </>
  );
}
