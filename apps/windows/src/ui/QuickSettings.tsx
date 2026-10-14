// The part of the settings that is the drop-down terminal: the shortcut that
// calls it from any program, changed by pressing the new one, and whether the
// system gave it. A shortcut the system refuses (another program has it) is
// said here for as long as it is so, with one that is free to take instead.

import { useEffect, useState } from "preact/hooks";
import { chooseShortcut, quick, refreshQuick, toggleQuick } from "../quick/main";
import { DEFAULT_SHORTCUT, shortcutFromChord, shortcutKeys, shortcutText, whyUnusable } from "../quick/shortcut";
import { isMac } from "../store";

function Caps(props: { shortcut: string }) {
  return (
    <span class="caps quick-caps">
      {shortcutKeys(props.shortcut, isMac.value).map((key, i) => (
        <kbd key={i} class="cap">
          {key}
        </kbd>
      ))}
    </span>
  );
}

const sentence = (text: string) => `${text.charAt(0).toUpperCase()}${text.slice(1)}.`;

export function QuickSettings() {
  const status = quick.value;
  const [listening, setListening] = useState(false);
  const [said, setSaid] = useState<string | null>(null);

  useEffect(() => {
    void refreshQuick();
  }, []);

  const choose = async (shortcut: string) => {
    setSaid(await chooseShortcut(shortcut));
  };

  // The next chord is the shortcut: it goes to this and to nothing else, not to the card behind or the terminal.
  useEffect(() => {
    if (!listening) return;
    const onKey = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      if (event.code === "Escape") {
        setListening(false);
        return;
      }
      const shortcut = shortcutFromChord(event);
      if (shortcut === null) return;
      const why = whyUnusable(shortcut);
      if (why) {
        setSaid(sentence(why));
        return;
      }
      setListening(false);
      void choose(shortcut);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [listening]);

  const shortcut = status?.shortcut ?? DEFAULT_SHORTCUT;
  return (
    <fieldset>
      <legend>Janela rápida</legend>
      <div class="choice">
        <span class="choice-name">
          {listening ? <span class="quick-listening">Aperte o novo atalho… (Esc cancela)</span> : <Caps shortcut={shortcut} />}
        </span>
        <button
          onClick={() => {
            setSaid(null);
            setListening(!listening);
          }}
        >
          {listening ? "Cancelar" : "Alterar…"}
        </button>
        {!listening && shortcut !== DEFAULT_SHORTCUT && <button onClick={() => void choose(DEFAULT_SHORTCUT)}>Usar o padrão</button>}
        {!listening && <button onClick={toggleQuick}>Abrir agora</button>}
      </div>
      {status && !status.registered && (
        <p class="hint quick-problem">
          {status.problem ?? "O sistema não liberou esse atalho."}
          {status.suggestion && (
            <>
              {" "}
              <button class="small-button" onClick={() => void choose(status.suggestion!)}>
                Usar {shortcutText(status.suggestion, isMac.value)}
              </button>
            </>
          )}
        </p>
      )}
      {said && <p class="hint quick-problem">{said}</p>}
      <p class="hint">
        Um terminal que desce do topo da tela, em qualquer programa, e sai quando você clica fora. Vive no workspace “quick”, que a barra
        lateral não mostra.
      </p>
    </fieldset>
  );
}
