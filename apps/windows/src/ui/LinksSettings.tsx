// The part of the settings that is the claude-cli:// links: whether the "open in Claude Code" links of claude.ai
// open a tab here (deepLinks.ts). Off until the person turns it on; turning it off puts back what was there.

import { useEffect } from "preact/hooks";
import { links, linksBusy, linksHint, refreshLinks, setLinks } from "../deepLinks";
import { isWindows } from "../store";

export function LinksSettings() {
  useEffect(() => {
    void refreshLinks();
  }, []);
  // The registration is the Windows registry's: elsewhere the macOS app has its own (`keep links instalar`).
  if (!isWindows.value) return null;
  const reading = links.value;
  const on = reading?.kind === "status" && reading.status.on;
  const answered = reading?.kind === "status" || reading?.kind === "error";
  return (
    <fieldset>
      <legend>Links do Claude Code</legend>
      <div class="choice">
        <span class="choice-name">Abrir links claude-cli:// no Keep</span>
        <div class="segmented">
          {([[false, "Desligado"], [true, "Ligado"]] as const).map(([value, label]) => (
            <button key={String(value)} class={answered && on === value ? "on" : ""} disabled={linksBusy.value || reading === null || reading.kind === "unavailable"} onClick={() => void setLinks(value)}>
              {label}
            </button>
          ))}
        </div>
      </div>
      <p class={reading?.kind === "error" || (reading?.kind === "status" && reading.status.on && !reading.status.complete) ? "hint quick-problem" : "hint"}>{linksHint(reading)}</p>
    </fieldset>
  );
}
