import { describe, expect, it } from "vitest";
import { errMsg } from "./errors";

describe("errMsg", () => {
  it("uses the message of an Error", () => {
    expect(errMsg(new Error("boom"))).toBe("boom");
  });

  it("stringifies non-Error values", () => {
    expect(errMsg("plain string")).toBe("plain string");
    expect(errMsg(42)).toBe("42");
    expect(errMsg(null)).toBe("null");
    expect(errMsg(undefined)).toBe("undefined");
  });

  it("uses the subclass message for Error subclasses", () => {
    class MyError extends Error {}
    expect(errMsg(new MyError("nope"))).toBe("nope");
  });
});
