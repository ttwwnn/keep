// The window's title (the macOS app's MainWindowController.updateTitle): the
// tab in front. It is what the taskbar, Alt+Tab and the window list show of
// a window that draws no title bar of its own, so it also says which
// workspace the tab is in, unless the tab is called the same.

/** At most this many characters: the system's lists cut a long title anyway, and a program can set a very long one. */
const LONGEST = 120;

export function windowTitle(workspace: string | null, tab: string | null): string {
  const named = tab?.trim() || null;
  const where = workspace?.trim() || null;
  const title = named && where && named !== where ? `${named} — ${where}` : (named ?? where ?? "Keep");
  return title.length > LONGEST ? `${title.slice(0, LONGEST - 1)}…` : title;
}
