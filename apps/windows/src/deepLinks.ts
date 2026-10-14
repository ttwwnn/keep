// The claude-cli:// links ("open in Claude Code" on claude.ai) opening a tab here: what the core says of them, and
// the switch in Settings. The registration is the core's (`keep links`, crates/keep-restauro/src/protocolo.rs): it
// writes the link's handler under the user's own key of the registry and asks Claude Code not to write its own back,
// and undoes both when asked. Nothing here decides what a link may do; that is checked where it is opened.

import { signal } from "@preact/signals";
import { iaAsk, type CoreAnswer } from "./api";
import { log } from "./store";

export interface LinksStatus {
  /** The registry sends the links to this program. */
  on: boolean;
  /** Everything a link needs is in place; on but not complete means Claude Code may take the registration back. */
  complete: boolean;
  /** What is not in place, in the core's words. */
  missing: string[];
}

export type LinksReading =
  | { kind: "status"; status: LinksStatus }
  /** The core refused or failed; why, in words. */
  | { kind: "error"; message: string }
  /** No core here, or one that predates the links. */
  | { kind: "unavailable" };

const isObject = (value: unknown): value is Record<string, unknown> => typeof value === "object" && value !== null && !Array.isArray(value);

/** What `keep links conferir|instalar|remover --json` answered. */
export function readLinks(answer: CoreAnswer | null): LinksReading {
  if (answer === null) return { kind: "unavailable" };
  // An exit 2 with no JSON of its own is a core that does not know the command.
  if (answer.ok === false && answer.motivo === "uso") return { kind: "unavailable" };
  if (answer.ok === false) return { kind: "error", message: typeof answer.detalhe === "string" && answer.detalhe ? answer.detalhe : "o Keep não conseguiu" };
  if (typeof answer.ligado !== "boolean" || !Array.isArray(answer.itens)) return { kind: "unavailable" };
  const missing = answer.itens.flatMap((item) => (isObject(item) && item.ok === false && typeof item.descricao === "string" ? [item.descricao] : []));
  return { kind: "status", status: { on: answer.ligado, complete: answer.completo === true, missing } };
}

/** What the switch says under it. */
export function linksHint(reading: LinksReading | null): string {
  const about =
    "Os links “abrir no Claude Code” do claude.ai abrem uma aba do Keep, na pasta do link e com o texto já na caixa do Claude, em vez de uma janela do Windows Terminal. " +
    "Ligar registra o Keep como o programa desses links na sua conta do Windows (sem administrador) e pede ao Claude Code que não refaça o registro; desligar devolve os dois ao que eram.";
  if (reading === null) return about;
  if (reading.kind === "unavailable") return "Esta versão do Keep não tem o programa dos links.";
  if (reading.kind === "error") return reading.message;
  const { status } = reading;
  if (status.on && !status.complete) return `Ligado, mas incompleto: ${status.missing.join("; ")}. Ligue de novo para refazer.`;
  return about;
}

/** The core's last word on the links; null before the first look. */
export const links = signal<LinksReading | null>(null);
/** A change under way, so the switch is not pressed twice. */
export const linksBusy = signal(false);

async function ask(args: string[]): Promise<LinksReading> {
  try {
    return readLinks(await iaAsk(args));
  } catch (error) {
    log(`links claude-cli://: ${String(error)}`);
    return { kind: "unavailable" };
  }
}

export async function refreshLinks(): Promise<void> {
  links.value = await ask(["links", "conferir", "--json"]);
}

/** Turn the links on or off; they read back as the core left them. */
export async function setLinks(on: boolean): Promise<void> {
  if (linksBusy.value) return;
  linksBusy.value = true;
  try {
    links.value = await ask(["links", on ? "instalar" : "remover", "--json"]);
  } finally {
    linksBusy.value = false;
  }
}
