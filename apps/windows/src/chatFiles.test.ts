import { describe, expect, it } from "vitest";
import type { ChatFile } from "./api";
import {
  draggedWidth,
  modifiedText,
  NEWEST_FIRST,
  nextOrder,
  PANEL_SHUT,
  panelMaximum,
  releasedWidth,
  sizeText,
  sortFiles,
} from "./chatFiles";
import { SIDEBAR_MIN, sidebarFloor } from "./state";

const file = (name: string, type: string, modified: number, size: number): ChatFile => ({
  path: `C:\\x\\${name}${type ? "." + type : ""}`,
  name,
  type,
  modified,
  size,
  dir: false,
});

// As the app sends them: newest first.
const files = [
  file("relatorio 10", "pdf", 300, 5_000),
  file("Relatorio 2", "html", 200, 900),
  file("arquivo", "csv", 100, 70_000),
];
const names = (list: ChatFile[]) => list.map((f) => f.name);

describe("the order of the files panel", () => {
  it("is the newest first until a title is clicked", () => {
    expect(names(sortFiles(files, NEWEST_FIRST))).toEqual(["relatorio 10", "Relatorio 2", "arquivo"]);
  });

  it("a click on a title sorts by it from the start, a second one turns the arrow", () => {
    const byName = nextOrder(NEWEST_FIRST, "name");
    expect(byName).toEqual({ column: "name", ascending: true });
    // Numbers in a name in their order, and capitals as small letters.
    expect(names(sortFiles(files, byName))).toEqual(["arquivo", "Relatorio 2", "relatorio 10"]);
    const back = nextOrder(byName, "name");
    expect(back).toEqual({ column: "name", ascending: false });
    expect(names(sortFiles(files, back))).toEqual(["relatorio 10", "Relatorio 2", "arquivo"]);
    expect(nextOrder(NEWEST_FIRST, "modified")).toEqual({ column: "modified", ascending: true });
  });

  it("by type and by size", () => {
    expect(names(sortFiles(files, { column: "type", ascending: true }))).toEqual(["arquivo", "Relatorio 2", "relatorio 10"]);
    expect(names(sortFiles(files, { column: "size", ascending: false }))).toEqual(["arquivo", "relatorio 10", "Relatorio 2"]);
  });

  it("rows that tie keep the order they came in", () => {
    const same = [file("b", "pdf", 1, 1), file("a", "pdf", 2, 1)];
    expect(names(sortFiles(same, { column: "type", ascending: true }))).toEqual(["b", "a"]);
    expect(names(sortFiles(same, { column: "type", ascending: false }))).toEqual(["b", "a"]);
  });

  it("does not change the list it was given", () => {
    const copy = [...files];
    sortFiles(files, { column: "name", ascending: true });
    expect(files).toEqual(copy);
  });
});

describe("a row's texts", () => {
  it("modified as day/month hour:minute", () => {
    expect(modifiedText(new Date(2026, 10, 7, 9, 5).getTime())).toBe("07/11 09:05");
    expect(modifiedText(new Date(2026, 0, 31, 23, 59).getTime())).toBe("31/01 23:59");
  });

  it("the size in thousands of bytes, with a decimal comma", () => {
    expect(sizeText(0)).toBe("0 bytes");
    expect(sizeText(1)).toBe("1 byte");
    expect(sizeText(999)).toBe("999 bytes");
    expect(sizeText(12_345)).toBe("12 KB");
    expect(sizeText(1_234_567)).toBe("1,2 MB");
    expect(sizeText(2_000_000)).toBe("2 MB");
    expect(sizeText(999_999)).toBe("1 MB");
    expect(sizeText(3_400_000_000)).toBe("3,4 GB");
  });
});

describe("the width of the files panel", () => {
  it("is at most a third of the screen", () => {
    expect(panelMaximum(1920)).toBe(640);
    expect(draggedWidth(360, 500, 640)).toBe(640);
    expect(draggedWidth(360, -400, 640)).toBe(0);
    expect(draggedWidth(360, 40, 640)).toBe(400);
  });

  it("let go narrower than 120 px it shuts, keeping the width chosen before", () => {
    expect(releasedWidth(PANEL_SHUT - 1, 360)).toEqual({ open: false, chosen: 360 });
    expect(releasedWidth(PANEL_SHUT, 360)).toEqual({ open: true, chosen: PANEL_SHUT });
    expect(releasedWidth(412.4, 360)).toEqual({ open: true, chosen: 412 });
  });
});

describe("the narrowest sidebar", () => {
  it("holds its top row whole, with the row's padding and the card's margin and rim", () => {
    // The zoom (88) and the button (26) six apart, the battery (40) six before: 166 + 10 of padding + 18 of card.
    expect(sidebarFloor(166, 10)).toBe(SIDEBAR_MIN);
    expect(sidebarFloor(200.2, 10)).toBe(229);
    // A zoomed row is wider, and so is the floor.
    expect(sidebarFloor(260, 79)).toBe(357);
  });
});
