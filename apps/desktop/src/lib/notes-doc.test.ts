import { describe, expect, it } from "vitest";
import {
  clearCell,
  emptyTab,
  moveCursor,
  setCell,
  setStrings,
  setWidth,
  type TabBlock,
} from "./notes-doc";

const tab = (rows: string[]): TabBlock => ({ kind: "tab", strings: rows.length, width: rows[0].length, rows });

describe("emptyTab", () => {
  it("fills a strings×width grid of dashes", () => {
    expect(emptyTab(3, 4)).toEqual({ kind: "tab", strings: 3, width: 4, rows: ["----", "----", "----"] });
  });
});

describe("setStrings (top-anchored growth)", () => {
  it("prepends blank rows at the top, keeping bottom content", () => {
    const t = tab(["11--", "22--"]);
    expect(setStrings(t, 4).rows).toEqual(["----", "----", "11--", "22--"]);
  });
  it("shrinks from the top, erasing the highest rows", () => {
    const t = tab(["aa--", "bb--", "cc--"]);
    expect(setStrings(t, 1).rows).toEqual(["cc--"]);
  });
  it("clamps to [1,12]", () => {
    expect(setStrings(tab(["----"]), 0).strings).toBe(1);
    expect(setStrings(tab(["----"]), 99).strings).toBe(12);
  });
});

describe("setWidth (right-anchored)", () => {
  it("appends dashes on grow, keeping content", () => {
    expect(setWidth(tab(["12--"]), 6).rows).toEqual(["12----"]);
  });
  it("truncates from the right on shrink", () => {
    expect(setWidth(tab(["12345-"]), 3).rows).toEqual(["123"]);
  });
  it("clamps to [1,256]", () => {
    expect(setWidth(tab(["----"]), 0).width).toBe(1);
  });
});

describe("setCell / clearCell (overtype)", () => {
  it("writes one char without changing width", () => {
    const t = setCell(tab(["----", "----"]), 1, 2, "7");
    expect(t.rows).toEqual(["----", "--7-"]);
    expect(t.width).toBe(4);
  });
  it("clears a cell back to dash", () => {
    expect(clearCell(tab(["--7-"]), 0, 2).rows).toEqual(["----"]);
  });
  it("ignores out-of-bounds", () => {
    const t = tab(["----"]);
    expect(setCell(t, 5, 5, "9")).toEqual(t);
  });
});

describe("moveCursor", () => {
  const t = tab(["----", "----", "----"]);
  it("clamps within the grid", () => {
    expect(moveCursor(t, { row: 0, col: 0 }, "up")).toEqual({ row: 0, col: 0 });
    expect(moveCursor(t, { row: 0, col: 0 }, "down")).toEqual({ row: 1, col: 0 });
    expect(moveCursor(t, { row: 0, col: 3 }, "right")).toEqual({ row: 0, col: 3 });
    expect(moveCursor(t, { row: 1, col: 1 }, "left")).toEqual({ row: 1, col: 0 });
  });
});
