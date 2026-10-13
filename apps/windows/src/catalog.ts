// What a list of themes or faces does with what is typed over it.

import { fuzzyScore } from "./text";

/** The names that fit a query, best fit first; every name, in their order, for none. */
export function filterNames(names: readonly string[], query: string): string[] {
  if (query.trim() === "") return [...names];
  const scored: { name: string; score: number }[] = [];
  for (const name of names) {
    const score = fuzzyScore(query, name);
    if (score !== undefined) scored.push({ name, score });
  }
  // Array.sort is stable: names with the same fit stay in the list's order.
  return scored.sort((a, b) => a.score - b.score).map((entry) => entry.name);
}
