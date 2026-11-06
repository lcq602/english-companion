import { describe, expect, it } from "vitest";
import { isMostlyEnglish, splitWords } from "./words";

describe("splitWords", () => {
  it("splits words and punctuation", () => {
    const tokens = splitWords("Hello, world!");
    const words = tokens.filter((t) => t.isWord).map((t) => t.text);
    expect(words).toEqual(["Hello", "world"]);
    expect(tokens.map((t) => t.text).join("")).toBe("Hello, world!");
  });
  it("keeps apostrophes in words", () => {
    const words = splitWords("don't stop").filter((t) => t.isWord).map((t) => t.text);
    expect(words).toEqual(["don't", "stop"]);
  });
});

describe("isMostlyEnglish", () => {
  it("recognizes English text", () => expect(isMostlyEnglish("I use Spring Boot")).toBe(true));
  it("recognizes Chinese text", () => expect(isMostlyEnglish("我正在学习英语")).toBe(false));
});
