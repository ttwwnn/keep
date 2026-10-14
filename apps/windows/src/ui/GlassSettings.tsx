// The translucent ground in the settings: how much of the window's ground is painted, and the material behind it.
// Off (a whole ground) unless asked; only Windows has it, where the Mac gets it from Ghostty's own config.
// What the choices mean, and what the system gives, is glass.ts's.

import { setGlassBlur, setGlassOpacity } from "../appearance";
import * as api from "../api";
import { acrylicDrags, blurChoices, needsRestart, OPACITY_MIN, type GlassBlur } from "../glass";
import { gui, info, say } from "../store";

const BLUR_LABELS: Record<GlassBlur, string> = { none: "Nenhum", mica: "Mica", acrylic: "Acrílico" };

export function GlassSettings() {
  const startup = info.value;
  // The Mac's translucency is Ghostty's, in the config of whoever uses it; here it is the Windows build's.
  if (!startup || startup.platform !== "windows") return null;
  const glass = gui.value.glass;
  const percent = Math.round(glass.opacity * 100);
  const translucent = glass.opacity < 1;
  const choices = blurChoices(startup.platform, startup.windowsBuild);
  const restart = needsRestart(glass, startup.windowTransparent);

  return (
    <fieldset class="glass-settings">
      <legend>Fundo translúcido</legend>
      <div class="field-row">
        <span class="unit">Opacidade</span>
        <input
          class="glass-slider"
          type="range"
          min={OPACITY_MIN * 100}
          max={100}
          step={5}
          value={percent}
          aria-label="Opacidade do fundo"
          onInput={(e) => setGlassOpacity(Number((e.currentTarget as HTMLInputElement).value) / 100)}
        />
        <span class="unit glass-percent">{percent}%</span>
      </div>
      <p class="hint">100% deixa o fundo opaco, como sempre foi. O texto nunca fica translúcido.</p>
      {choices.length > 1 && (
        <div class="field-row">
          <span class="unit">Desfoque</span>
          <div class="segmented">
            {choices.map((blur) => (
              <button
                key={blur}
                class={glass.blur === blur ? "on" : ""}
                disabled={!translucent}
                onClick={() => setGlassBlur(blur)}
              >
                {BLUR_LABELS[blur]}
              </button>
            ))}
          </div>
        </div>
      )}
      {translucent && glass.blur === "acrylic" && acrylicDrags(startup.platform, startup.windowsBuild) && (
        <p class="hint">No Windows 10 o Acrílico pode travar ao arrastar ou redimensionar a janela.</p>
      )}
      {restart && (
        <div class="field-row glass-restart">
          <span class="hint">A janela só pode ficar translúcida quando é aberta assim: reabra o Keep. As abas continuam.</span>
          <button
            class="small-button"
            onClick={() => void api.restartApp().catch((error) => say(`Não consegui reabrir o Keep: ${String(error)}`))}
          >
            Reiniciar o Keep
          </button>
        </div>
      )}
    </fieldset>
  );
}
