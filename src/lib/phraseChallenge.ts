export interface Challenge {
  position: number;
  options: string[];
  correctWord: string;
}

// Picks 3 unique word positions and builds a 4-option challenge for each.
// Decoy options are drawn from the phrase itself so no external word list is needed.
export function generateWordChallenges(words: string[]): Challenge[] {
  const indices = pickThreeIndices(words.length);
  return indices.map((idx) => {
    const correctWord = words[idx];
    const decoys = pickDecoys(words, idx, 3);
    const options = shuffle([correctWord, ...decoys]);
    return { position: idx + 1, options, correctWord };
  });
}

function pickThreeIndices(length: number): number[] {
  const pool = Array.from({ length }, (_, i) => i);
  const picked: number[] = [];
  while (picked.length < 3 && pool.length > 0) {
    const i = Math.floor(Math.random() * pool.length);
    picked.push(pool.splice(i, 1)[0]);
  }
  return picked;
}

function pickDecoys(words: string[], excludeIdx: number, count: number): string[] {
  const candidates = words
    .map((w, i) => ({ w, i }))
    .filter(({ i, w }) => i !== excludeIdx && w !== words[excludeIdx])
    .map(({ w }) => w);
  const decoys: string[] = [];
  const pool = [...candidates];
  while (decoys.length < count && pool.length > 0) {
    const i = Math.floor(Math.random() * pool.length);
    decoys.push(pool.splice(i, 1)[0]);
  }
  return decoys;
}

function shuffle<T>(arr: T[]): T[] {
  const out = [...arr];
  for (let i = out.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [out[i], out[j]] = [out[j], out[i]];
  }
  return out;
}
