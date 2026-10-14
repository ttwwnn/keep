// A shortcut drawn as keys: written as `Ctrl+Shift+K` it is a string to be
// parsed, drawn as three caps it is three things to be pressed. The eye counts
// them without reading.

import { capsFor } from "../palette/caps";
import { isMac } from "../store";

export function KeyCaps(props: { keys: readonly string[] }) {
  if (props.keys.length === 0) return null;
  return (
    <span class="caps">
      {capsFor(props.keys, isMac.value).map((key, i) => (
        <kbd key={i} class="cap">
          {key}
        </kbd>
      ))}
    </span>
  );
}
