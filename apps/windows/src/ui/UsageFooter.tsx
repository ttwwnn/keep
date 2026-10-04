// The usage footer at the bottom of the sidebar: every signed-in account in
// the order of priority, its windows as bars, and when each starts over —
// the macOS app's footer (UI/UsageFooter.swift), with its rules and words.
//
// Set apart by a rule and quiet until something is near its limit: the bars
// are the ink of resting text, and only a window past 75% takes a colour —
// amber, then red from 90. The figure is always written beside the bar, so
// the colour is never the only thing saying it.

import {
  STALE_AFTER_S,
  accountHelp,
  accountName,
  barHelp,
  isUsedBy,
  level,
  lineId,
  noteText,
  orderKeyOf,
  percentText,
  untilText,
  type UsageLine,
  type UsageWindow,
} from "../ia";
import {
  footerFolded,
  iaAvailable,
  measureNow,
  measuring,
  moveOrder,
  selectedAccount,
  toggleFooter,
  usageClock,
  usageLines,
  usageNotice,
} from "../iaStore";
import { openSignInMenu } from "./AccountMenu";
import * as icon from "./icons";

/** ▲ and ▼: one place up or down. The first has nowhere up to go and the last nowhere down; each keeps the other's place. */
function Arrows(props: { line: UsageLine; index: number; count: number }) {
  const { line, index, count } = props;
  const name = accountName(line.account);
  const arrow = (up: boolean, shown: boolean) => (
    <button
      class={`usage-arrow ${shown ? "" : "hidden"}`}
      title={up ? "Subir na ordem de prioridade" : "Descer na ordem de prioridade"}
      aria-label={`${up ? "Subir" : "Descer"} ${name}`}
      aria-hidden={!shown}
      tabIndex={shown ? 0 : -1}
      onClick={() => shown && moveOrder(line, up ? -1 : 1)}
    >
      {up ? <icon.Up /> : <icon.Down />}
    </button>
  );
  return (
    <span class="usage-arrows">
      {arrow(true, index > 0)}
      {arrow(false, index < count - 1)}
    </span>
  );
}

function Bar(props: { window: UsageWindow; name: string; nowMs: number }) {
  const { window, name, nowMs } = props;
  const lvl = level(window.percent);
  const until = untilText(window.resetsAt, nowMs);
  const width = Math.min(100, Math.max(0, window.percent));
  return (
    <div
      class="usage-bar"
      title={barHelp(window, nowMs)}
      role="meter"
      aria-valuenow={Math.round(window.percent)}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-label={`${name} ${window.label} ${percentText(window.percent)}${until ? ` reinicia em ${until}` : ""}`}
    >
      <span class="usage-bar-label">{window.label}</span>
      <span class="usage-track">
        <span class={`usage-fill ${lvl}`} style={{ width: `max(2px, ${width}%)` }} />
      </span>
      <span class={`usage-percent ${lvl}`}>{percentText(window.percent)}</span>
      <span class="usage-until">{until ?? ""}</span>
    </div>
  );
}

/** "2 · Claude · reserva": the place in the order, which the arrows change, then the name. */
function Heading(props: { text: string; index: number; used: boolean }) {
  return (
    <span class={`usage-name ${props.used ? "used" : ""}`}>
      <span class="usage-place">{props.index + 1} · </span>
      {props.text}
    </span>
  );
}

function Block(props: { line: UsageLine; index: number; count: number; nowMs: number }) {
  const { line, index, count, nowMs } = props;
  const selected = selectedAccount.value;
  const used = isUsedBy(line.account, selected.key, selected.running);
  const stale = line.problem !== null || line.measuredAt === null || nowMs / 1000 - line.measuredAt > STALE_AFTER_S;
  const help = accountHelp(line, nowMs, used);
  const note = noteText(line, nowMs);
  const name = accountName(line.account);
  return (
    <div class="usage-block" data-account={orderKeyOf(line.account)}>
      <div class="usage-title-row">
        <span class="usage-title" title={help}>
          <Heading text={name} index={index} used={used} />
          {used && <span class="usage-live" title="Conta usada nesta aba" aria-label="Conta usada nesta aba" />}
        </span>
        <Arrows line={line} index={index} count={count} />
      </div>
      {/* Which login this is, written out: the name says which one the order means, the address which one the service does. */}
      {line.account.email && (
        <div class="usage-email" title={help}>
          {line.account.email}
        </div>
      )}
      {line.reading && line.reading.windows.length > 0 && (
        <div class={`usage-bars ${stale ? "stale" : ""}`}>
          {line.reading.windows.map((w, i) => (
            <Bar key={`${w.label}${i}`} window={w} name={name} nowMs={nowMs} />
          ))}
        </div>
      )}
      {note && <div class={`usage-note ${line.reading?.limitReached ? "critical" : ""}`}>{note}</div>}
    </div>
  );
}

/**
 * Folded: the account by its address — what tells two logins of one service
 * apart — and the figures of its first two windows, each in its own colour.
 */
function Compact(props: { line: UsageLine; index: number; count: number }) {
  const { line, index, count } = props;
  const selected = selectedAccount.value;
  const used = isUsedBy(line.account, selected.key, selected.running);
  const windows = (line.reading?.windows ?? []).slice(0, 2);
  const name = accountName(line.account);
  const help = [name, ...windows.map((w) => `${w.title}: ${percentText(w.percent)}`)].join("\n");
  return (
    <div class="usage-compact" data-account={orderKeyOf(line.account)}>
      <span class="usage-compact-name" title={help}>
        <Heading text={line.account.email ?? name} index={index} used={used} />
      </span>
      <span class="usage-compact-figures" aria-label={`${name} ${windows.map((w) => percentText(w.percent)).join(" · ")}`}>
        {windows.length === 0 ? (
          <span class="faint">—</span>
        ) : (
          windows.map((w, i) => (
            <span key={i}>
              {i > 0 && <span class="faint"> · </span>}
              <span class={`usage-percent-inline ${level(w.percent)}`}>{percentText(w.percent)}</span>
            </span>
          ))
        )}
      </span>
      <Arrows line={line} index={index} count={count} />
    </div>
  );
}

export function UsageFooter() {
  if (!iaAvailable.value) return null;
  const lines = usageLines.value;
  const folded = footerFolded.value;
  const nowMs = usageClock.value;
  return (
    <section class="usage-footer" aria-label="Consumo de IA">
      <div class="usage-head">
        <button
          class="usage-fold"
          title={folded ? "Mostrar as janelas de cada conta" : "Uma linha por conta"}
          aria-expanded={!folded}
          onClick={toggleFooter}
        >
          <icon.Chevron open={!folded} />
          <span>Consumo de IA</span>
        </button>
        <span class="usage-spacer" />
        <button
          class="usage-icon"
          title="Entrar em outra conta"
          aria-label="Entrar em outra conta"
          aria-haspopup="menu"
          onMouseDown={(e) => e.stopPropagation()}
          onClick={(e) => openSignInMenu(e.currentTarget as HTMLElement)}
        >
          <icon.Plus />
        </button>
        <button
          class={`usage-icon ${measuring.value ? "busy" : ""}`}
          title="Medir agora"
          aria-label="Medir agora"
          onClick={measureNow}
        >
          <icon.Refresh />
        </button>
      </div>
      {usageNotice.value && <div class="usage-notice">{usageNotice.value}</div>}
      <div class={`usage-lines ${folded ? "folded" : ""}`}>
        {lines.length === 0 && <div class="usage-empty">Nenhuma conta de IA ainda: entre numa pelo +.</div>}
        {lines.map((line, i) =>
          folded ? (
            <Compact key={lineId(line)} line={line} index={i} count={lines.length} />
          ) : (
            <Block key={lineId(line)} line={line} index={i} count={lines.length} nowMs={nowMs} />
          ),
        )}
      </div>
    </section>
  );
}
