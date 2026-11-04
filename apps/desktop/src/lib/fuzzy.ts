/**
 * Small fuzzy matcher for the command palette: all query characters must appear in order.
 * Scores favour matches at word starts and consecutive runs. Returns null when there is no match.
 */
export function fuzzyScore(query: string, text: string): number | null {
  const q = query.trim().toLowerCase()
  if (!q) return 0
  const t = text.toLowerCase()
  let score = 0
  let ti = 0
  let prevMatch = -2
  for (const ch of q) {
    if (ch === ' ') continue
    const found = t.indexOf(ch, ti)
    if (found === -1) return null
    const atWordStart = found === 0 || /[\s\-_/.:·]/.test(t[found - 1] ?? '')
    score += 1
    if (atWordStart) score += 4
    if (found === prevMatch + 1) score += 3
    score -= Math.min(found - ti, 6) * 0.1
    prevMatch = found
    ti = found + 1
  }
  // Prefer shorter targets for equal matches.
  return score - t.length * 0.01
}

export function fuzzyFilter<T>(items: T[], query: string, text: (item: T) => string, limit = 50): T[] {
  if (!query.trim()) return items.slice(0, limit)
  return items
    .map((item) => ({ item, score: fuzzyScore(query, text(item)) }))
    .filter((x): x is { item: T; score: number } => x.score !== null)
    .sort((a, b) => b.score - a.score)
    .slice(0, limit)
    .map((x) => x.item)
}
