// The files panel beside a Claude Code conversation: what it made, what the
// person sent, and the Downloads folder, each a table sorted by a click on a
// column's title. A port of ChatFilesPanel.swift and of the panel's part of
// MainWindowController.swift in the macOS app. Which files go in each list is
// decided in the app (src-tauri `chat_files.rs`); here are the order, the
// texts of a row and the panel's width.

import { signal } from "@preact/signals";
import type { ChatFile } from "./api";

export type Column = "name" | "type" | "modified" | "size";

/** The order of the three tables: one for all, newest first until a title is clicked. */
export interface Order {
  column: Column;
  ascending: boolean;
}

export const NEWEST_FIRST: Order = { column: "modified", ascending: false };

/** A click on a column's title: the same column turns its arrow, another sorts by itself from the start. */
export function nextOrder(order: Order, column: Column): Order {
  return order.column === column ? { column, ascending: !order.ascending } : { column, ascending: true };
}

const collator = new Intl.Collator("pt-BR", { numeric: true, sensitivity: "base" });

/** The rows in an order; rows that tie keep the order they came in (newest first). */
export function sortFiles(files: readonly ChatFile[], order: Order): ChatFile[] {
  const sign = order.ascending ? 1 : -1;
  const compare = (a: ChatFile, b: ChatFile): number => {
    switch (order.column) {
      case "name":
        return collator.compare(a.name, b.name);
      case "type":
        return collator.compare(a.type, b.type);
      case "modified":
        return a.modified - b.modified;
      case "size":
        return a.size - b.size;
    }
  };
  return files
    .map((file, at) => ({ file, at }))
    .sort((a, b) => sign * compare(a.file, b.file) || a.at - b.at)
    .map((entry) => entry.file);
}

const two = (n: number) => String(n).padStart(2, "0");

/** When a file last changed, as the row says it: `17/11 09:05`. */
export function modifiedText(ms: number): string {
  const d = new Date(ms);
  return `${two(d.getDate())}/${two(d.getMonth() + 1)} ${two(d.getHours())}:${two(d.getMinutes())}`;
}

/** How big a file is, as the Mac's file manager says it (thousands of bytes, a decimal comma): `999 bytes`, `12 KB`, `1,2 MB`. */
export function sizeText(bytes: number): string {
  if (bytes < 1000) return bytes === 1 ? "1 byte" : `${bytes} bytes`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1000;
  let unit = 0;
  while (value >= 999.5 && unit < units.length - 1) {
    value /= 1000;
    unit++;
  }
  const text = unit === 0 ? String(Math.round(value)) : value.toFixed(1).replace(".", ",").replace(/,0$/, "");
  return `${text} ${units[unit]}`;
}

// ---------------------------------------------------------------- the width

export const PANEL_DEFAULT = 360;
/** Let go narrower than a row can be read in, the panel shuts, keeping the width it had. */
export const PANEL_SHUT = 120;

/** At most a third of the screen the window is on. */
export function panelMaximum(screenWidth: number): number {
  return Math.max(PANEL_SHUT, Math.floor(screenWidth / 3));
}

/** The width while its edge is dragged: anywhere from nothing to the maximum. */
export function draggedWidth(start: number, moved: number, maximum: number): number {
  return Math.max(0, Math.min(maximum, start + moved));
}

/** Where a drag of the edge ends: shut (the width chosen before kept), or open at the width it was let go at. */
export function releasedWidth(width: number, chosen: number): { open: boolean; chosen: number } {
  return width < PANEL_SHUT ? { open: false, chosen } : { open: true, chosen: Math.round(width) };
}

// ---------------------------------------------------------------- the window's

const WIDTH_KEY = "keep.filesPanelWidth";

function storedWidth(): number {
  try {
    const value = Number(localStorage.getItem(WIDTH_KEY));
    return Number.isFinite(value) && value >= PANEL_SHUT ? value : PANEL_DEFAULT;
  } catch {
    return PANEL_DEFAULT;
  }
}

/** Whether the person opened the panel in this window: it shows beside a Claude Code conversation only. */
export const filesOpen = signal(false);
/** The width the person chose, the same in every window and the next time the app opens. */
export const filesWidth = signal(storedWidth());

export function chooseFilesWidth(width: number): void {
  filesWidth.value = width;
  try {
    localStorage.setItem(WIDTH_KEY, String(width));
  } catch {
    // Remembered for this run only.
  }
}

export function toggleFiles(): void {
  filesOpen.value = !filesOpen.value;
}
