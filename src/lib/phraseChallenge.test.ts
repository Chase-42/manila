import { describe, it, expect } from "vitest";
import { generateWordChallenges } from "./phraseChallenge";

const WORDS = Array.from({ length: 24 }, (_, i) => `word${i}`);

describe("generateWordChallenges", () => {
  it("returns exactly 3 challenges", () => {
    const challenges = generateWordChallenges(WORDS);
    expect(challenges).toHaveLength(3);
  });

  it("each challenge has 4 options", () => {
    const challenges = generateWordChallenges(WORDS);
    for (const c of challenges) {
      expect(c.options).toHaveLength(4);
    }
  });

  it("correct word appears in options exactly once", () => {
    const challenges = generateWordChallenges(WORDS);
    for (const c of challenges) {
      const count = c.options.filter((o) => o === c.correctWord).length;
      expect(count).toBe(1);
    }
  });

  it("no two challenges target the same position", () => {
    const challenges = generateWordChallenges(WORDS);
    const positions = challenges.map((c) => c.position);
    const unique = new Set(positions);
    expect(unique.size).toBe(3);
  });

  it("position matches the correct word from the phrase", () => {
    const challenges = generateWordChallenges(WORDS);
    for (const c of challenges) {
      expect(c.correctWord).toBe(WORDS[c.position - 1]);
    }
  });

  it("positions are in range 1 to 24", () => {
    const challenges = generateWordChallenges(WORDS);
    for (const c of challenges) {
      expect(c.position).toBeGreaterThanOrEqual(1);
      expect(c.position).toBeLessThanOrEqual(24);
    }
  });
});
