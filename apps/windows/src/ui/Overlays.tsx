// What floats over the terminal: the picker, the search across every tab,
// the settings, and the questions the window asks before it ends something.
//
// Nothing is painted over the terminal behind a card (DESIGN.md): the card's
// shadow says it is above, and says it where the card is.

import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useEffect, useMemo, useRef, useState } from "preact/hooks";
import * as api from "../api";
import {
  activeView,
  answer,
  createWorkspace,
  dialog,
  gui,
  info,
  orderedWorkspaces,
  overlay,
  select,
  setAppearance,
  setFont,
  tabLabel,
  tabsByWorkspace,
  terminals,
  workspaceLabel,
  type DialogField,
} from "../store";
import { defaultFontFamily, DEFAULT_FONT_SIZE } from "../theme";
import { fuzzyScore, homeRelative, matchRanges } from "../text";

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

/** Keep a highlighted row in view as the arrows move it. */
function useScrollIntoView(index: number) {
  const list = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const row = list.current?.children[index] as HTMLElement | undefined;
    row?.scrollIntoView({ block: "nearest" });
  }, [index]);
  return list;
}

function Highlighted(props: { text: string; query: string }) {
  const ranges = matchRanges(props.query, props.text);
  if (ranges.length === 0) return <>{props.text}</>;
  const out = [];
  let at = 0;
  for (const [start, end] of ranges) {
    if (start > at) out.push(props.text.slice(at, start));
    out.push(<mark key={start}>{props.text.slice(start, end)}</mark>);
    at = end;
  }
  out.push(props.text.slice(at));
  return <>{out}</>;
}

interface PickerRow {
  kind: "workspace" | "tab" | "create";
  workspace: string;
  tab?: number;
  title: string;
  detail: string;
  score: number;
}

/** Go to any workspace or tab, by typing a few letters of it. */
function Picker() {
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const home = info.value?.home ?? "";
  const rows = useMemo<PickerRow[]>(() => {
    const out: PickerRow[] = [];
    for (const w of orderedWorkspaces.value) {
      const label = workspaceLabel(w.name);
      const ws = fuzzyScore(query, label);
      if (ws !== undefined) out.push({ kind: "workspace", workspace: w.name, title: label, detail: "workspace", score: ws });
      (tabsByWorkspace.value.get(w.name) ?? []).forEach((tab, i) => {
        const title = tabLabel(w.name, tab, i);
        const where = homeRelative(tab.root.cwd, home);
        const score = fuzzyScore(query, `${label} ${title} ${where}`);
        if (score !== undefined) {
          out.push({ kind: "tab", workspace: w.name, tab: tab.root.id, title, detail: `${label}${where ? ` · ${where}` : ""}`, score });
        }
      });
    }
    out.sort((a, b) => a.score - b.score);
    const name = query.trim();
    if (name && !orderedWorkspaces.value.some((w) => w.name === name)) {
      out.push({ kind: "create", workspace: name, title: `Criar o workspace “${name}”`, detail: "Enter", score: Infinity });
    }
    return out;
  }, [query, orderedWorkspaces.value, tabsByWorkspace.value, gui.value]);
  const list = useScrollIntoView(index);
  const input = useFocus<HTMLInputElement>();
  const go = (row: PickerRow | undefined) => {
    if (!row) return;
    close();
    if (row.kind === "create") void createWorkspace(row.workspace, null);
    else select(row.workspace, row.tab);
  };
  return (
    <div class="card picker" role="dialog" aria-label="Ir para">
      <input
        ref={input}
        class="card-input"
        value={query}
        placeholder="Ir para um workspace ou aba…"
        spellcheck={false}
        onInput={(e) => {
          setQuery((e.currentTarget as HTMLInputElement).value);
          setIndex(0);
        }}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === "Escape") close();
          else if (e.key === "ArrowDown") (e.preventDefault(), setIndex((i) => Math.min(rows.length - 1, i + 1)));
          else if (e.key === "ArrowUp") (e.preventDefault(), setIndex((i) => Math.max(0, i - 1)));
          else if (e.key === "Enter") go(rows[index]);
        }}
      />
      <div class="card-list" ref={list}>
        {rows.map((row, i) => (
          <button
            key={`${row.kind}${row.workspace}${row.tab ?? ""}`}
            class={`card-row ${i === index ? "selected" : ""} ${row.kind}`}
            onMouseMove={() => setIndex(i)}
            onClick={() => go(row)}
          >
            <span class="row-title">
              <Highlighted text={row.title} query={query} />
            </span>
            <span class="row-detail">{row.detail}</span>
          </button>
        ))}
        {rows.length === 0 && <div class="card-empty">Nada com esse nome.</div>}
      </div>
    </div>
  );
}

/** Search every tab's history, which the daemon holds. */
function SearchAll() {
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<api.SearchHit[]>([]);
  const [index, setIndex] = useState(0);
  const [busy, setBusy] = useState(false);
  const list = useScrollIntoView(index);
  const input = useFocus<HTMLInputElement>();
  useEffect(() => {
    const q = query.trim();
    if (!q) {
      setHits([]);
      return;
    }
    let cancelled = false;
    setBusy(true);
    const timer = setTimeout(() => {
      api
        .search(q, 60)
        .then((found) => !cancelled && (setHits(found), setIndex(0)))
        .catch(() => !cancelled && setHits([]))
        .finally(() => !cancelled && setBusy(false));
    }, 200);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [query]);
  const go = (hit: api.SearchHit | undefined) => {
    if (!hit) return;
    const tabs = tabsByWorkspace.value.get(hit.workspace) ?? [];
    const root = tabs.find((t) => t.panes.some((p) => p.id === hit.tab));
    close();
    if (!root) return;
    select(hit.workspace, root.root.id);
    // In front of the person, the match found again in the terminal itself.
    setTimeout(() => {
      const view = terminals.existing(hit.workspace, hit.tab);
      view?.search.findNext(hit.text.slice(hit.matchStart, hit.matchStart + hit.matchLen), {
        decorations: { matchOverviewRuler: "#888888", activeMatchColorOverviewRuler: "#ffaa00" },
      });
    }, 60);
  };
  const tabTitle = (hit: api.SearchHit) => {
    const tabs = tabsByWorkspace.value.get(hit.workspace) ?? [];
    const i = tabs.findIndex((t) => t.panes.some((p) => p.id === hit.tab));
    return i >= 0 ? tabLabel(hit.workspace, tabs[i], i) : `aba ${hit.tab}`;
  };
  return (
    <div class="card search-all" role="dialog" aria-label="Buscar em tudo">
      <input
        ref={input}
        class="card-input"
        value={query}
        placeholder="Buscar em todas as abas…"
        spellcheck={false}
        onInput={(e) => setQuery((e.currentTarget as HTMLInputElement).value)}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === "Escape") close();
          else if (e.key === "ArrowDown") (e.preventDefault(), setIndex((i) => Math.min(hits.length - 1, i + 1)));
          else if (e.key === "ArrowUp") (e.preventDefault(), setIndex((i) => Math.max(0, i - 1)));
          else if (e.key === "Enter") go(hits[index]);
        }}
      />
      <div class="card-list" ref={list}>
        {hits.map((hit, i) => (
          <button
            key={`${hit.workspace}${hit.tab}${hit.line}`}
            class={`card-row hit ${i === index ? "selected" : ""}`}
            onMouseMove={() => setIndex(i)}
            onClick={() => go(hit)}
          >
            <span class="row-detail">
              {workspaceLabel(hit.workspace)} · {tabTitle(hit)}
            </span>
            {hit.before.slice(-1).map((line, j) => (
              <span key={`b${j}`} class="hit-context">
                {line}
              </span>
            ))}
            <span class="hit-line">
              {hit.text.slice(0, hit.matchStart)}
              <mark>{hit.text.slice(hit.matchStart, hit.matchStart + hit.matchLen)}</mark>
              {hit.text.slice(hit.matchStart + hit.matchLen)}
            </span>
            {hit.after.slice(0, 1).map((line, j) => (
              <span key={`a${j}`} class="hit-context">
                {line}
              </span>
            ))}
          </button>
        ))}
        {query.trim() && !busy && hits.length === 0 && <div class="card-empty">Nada encontrado.</div>}
      </div>
    </div>
  );
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
      // The button that opened it closes it itself, on its click.
      if (!target.closest(".card") && !target.closest("[data-overlay-toggle]")) close();
    };
    window.addEventListener("mousedown", onDown);
    return () => window.removeEventListener("mousedown", onDown);
  }, [which]);
  return (
    <>
      {which === "picker" && <Picker />}
      {which === "search" && <SearchAll />}
      {which === "settings" && <Settings />}
      <DialogView />
    </>
  );
}
