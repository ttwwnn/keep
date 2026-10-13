// The hook the app sets up in Claude Code's settings, so that the Claude of a
// tab names the account the tab runs on after a hot switch (claudeHooks.ts,
// `keep ia conta-da-aba` in docs/ia.md). Done as the status line is
// (store.ts `setUpStatusLine`): on opening, in a file read and written back
// only if nothing changed meanwhile; what was done goes to the log.

import * as api from "./api";
import { planAccountHook } from "./claudeHooks";
import { info, log } from "./store";

/**
 * `UserPromptSubmit` → `keep ia conta-da-aba --gancho`, added to the settings
 * file of Claude Code when it has none like it: after the hooks the person
 * has, nothing else changed — and where Claude Code's folder is there; without
 * it Claude Code has not run here, and no file is made for it. Windows only:
 * elsewhere the app is for development, and its settings are somebody's own.
 */
export async function setUpAccountHook(): Promise<string> {
  if (info.value?.platform !== "windows") return "fora do Windows";
  let said: string;
  try {
    const settings = await api.claudeSettings();
    if (settings.keep === null) {
      said = "o keep.exe não está junto do app";
    } else if (settings.text === null && !settings.folder) {
      said = `sem a pasta do Claude Code (${settings.path}): ele ainda não rodou aqui`;
    } else {
      const plan = planAccountHook(settings.text, settings.keep);
      switch (plan.kind) {
        case "write":
          said = (await api.writeClaudeSettings(settings.text, plan.text))
            ? `gravado em ${settings.path}`
            : `${settings.path} mudou enquanto era lido: fica para a próxima abertura`;
          break;
        case "ours":
          said = "já é o do Keep";
          break;
        case "present":
          said = `${settings.path} já tem um gancho da conta da aba (${plan.command}): não foi tocado`;
          break;
        case "unreadable":
          said = `${settings.path} não foi tocado: ${plan.why}`;
          break;
      }
    }
  } catch (error) {
    said = `não deu para ler o settings.json: ${String(error)}`;
  }
  log(`gancho da conta da aba: ${said}`);
  return said;
}
