// What floats over the terminal: the picker (ui/Picker.tsx: Ir para, the
// palette, the searches), the settings with the lists of themes and faces they
// open, and the questions the window asks before it ends something.
//
// Nothing is painted over the terminal behind a card (DESIGN.md): the card's
// shadow says it is above, and says it where the card is.

import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useEffect, useRef, useState } from "preact/hooks";
import * as api from "../api";
import { activeView, answer, dialog, info, overlay, type DialogField } from "../store";
import { AppearanceSettings } from "./AppearanceSettings";
import { FontPicker } from "./FontPicker";
import { Picker } from "./Picker";
import { QuickSettings } from "./QuickSettings";
import { ThemePicker } from "./ThemePicker";

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
  const [shells, setShells] = useState<{ chosen: string; available: api.Shell[] } | null>(null);
  const card = useRef<HTMLDivElement>(null);
  useEffect(() => {
    void api.shells().then(setShells).catch(() => setShells({ chosen: "", available: [] }));
    // The keyboard comes in with it: Tab goes through its controls and Esc closes it, rather than both going to the terminal behind.
    card.current?.focus();
  }, []);
  return (
    <div
      ref={card}
      tabIndex={-1}
      class="card settings"
      role="dialog"
      aria-label="Configurações"
      onKeyDown={(e) => e.key === "Escape" && close()}
    >
      <h2>Configurações</h2>
      <div class="settings-body">
        <AppearanceSettings />
        <QuickSettings />
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
      </div>
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
      {which === "themes" && <ThemePicker />}
      {which === "fonts" && <FontPicker />}
      <DialogView />
    </>
  );
}
