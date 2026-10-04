import { gui, isMac, isWindows } from "../store";
import { ContextMenu } from "./common";
import { Overlays } from "./Overlays";
import { Sidebar } from "./Sidebar";
import { TerminalArea } from "./TerminalArea";
import { TitleBar } from "./TitleBar";

/** The window: the sidebar the full height, the strip and the terminals beside it. */
export function App() {
  return (
    <div class={`app ${isMac.value ? "mac" : ""} ${isWindows.value ? "windows" : ""}`}>
      {gui.value.sidebar.visible && <Sidebar />}
      <div class="main">
        <TitleBar />
        <TerminalArea />
      </div>
      <Overlays />
      <ContextMenu />
    </div>
  );
}
