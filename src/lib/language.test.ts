// Change date: 2026-08-04 | Change: Test translation direction detection | Iteration: 1
import { describe, expect, it } from "vitest";
import { detectDirection } from "./language";

describe("detectDirection", () => {
  it("detects English words as to-chinese", () => expect(detectDirection("Chinese study")).toBe("to-chinese"));
  it("detects English input", () => expect(detectDirection("I am learning English")).toBe("to-chinese"));
  it("detects CJK input", () => expect(detectDirection("我正在学习英语")).toBe("to-english"));
  it("returns mixed for balanced bilingual input", () => expect(detectDirection("学习 English")).toBe("mixed"));
  it("returns empty for whitespace", () => expect(detectDirection("  \n  ")).toBe("empty"));
});
