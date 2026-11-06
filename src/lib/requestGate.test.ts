import { describe, expect, it } from "vitest";
import { RequestGate } from "./requestGate";

describe("RequestGate", () => {
  it("only the latest request is current", () => {
    const gate = new RequestGate();
    const first = gate.next();
    const second = gate.next();
    expect(gate.isCurrent(first)).toBe(false);
    expect(gate.isCurrent(second)).toBe(true);
  });

  it("invalidate makes an in-flight request stale", () => {
    const gate = new RequestGate();
    const id = gate.next();
    gate.invalidate();
    expect(gate.isCurrent(id)).toBe(false);
  });

  it("a request after invalidation can complete normally", () => {
    const gate = new RequestGate();
    const stale = gate.next();
    gate.invalidate();
    const fresh = gate.next();
    expect(gate.isCurrent(stale)).toBe(false);
    expect(gate.isCurrent(fresh)).toBe(true);
  });
});
