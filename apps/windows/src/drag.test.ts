// How a list scrolls under a drag held near its edge: not at all until the
// pointer is close, faster as it nears the edge, and never faster than the most.
import { describe, expect, it } from "vitest";
import { scrollStep } from "./drag";

describe("the scrolling of a list under a drag", () => {
  it("is none while the pointer is further from the edge than the reach", () => {
    expect(scrollStep(36)).toBe(0);
    expect(scrollStep(200)).toBe(0);
  });
  it("is the most at the edge and past it, however far past", () => {
    expect(scrollStep(0)).toBe(14);
    expect(scrollStep(-1)).toBe(14);
    expect(scrollStep(-500)).toBe(14);
  });
  it("grows as the pointer nears the edge", () => {
    expect(scrollStep(35)).toBe(1);
    expect(scrollStep(18)).toBe(7);
    let before = 0;
    for (let distance = 36; distance >= 0; distance -= 3) {
      const now = scrollStep(distance);
      expect(now).toBeGreaterThanOrEqual(before);
      before = now;
    }
  });
  it("is told its reach and its most", () => {
    expect(scrollStep(10, 20, 10)).toBe(5);
    expect(scrollStep(-3, 20, 10)).toBe(10);
  });
});
