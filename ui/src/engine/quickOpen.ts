export type QuickOpenItem<T> = { key: string; value: T };

/** Recent paths first; everything else retains its engine-provided stable order. */
export function rankQuickOpen<T>(items: readonly QuickOpenItem<T>[], recent: readonly string[]): T[] {
  const rank = new Map(recent.map((key, index) => [key, index]));
  return items
    .map((item, order) => ({ ...item, order, rank: rank.get(item.key) ?? Number.MAX_SAFE_INTEGER }))
    .sort((a, b) => a.rank - b.rank || a.order - b.order)
    .map((item) => item.value);
}

export function evictMissingRecent(recent: readonly string[], existing: ReadonlySet<string>): string[] {
  return recent.filter((key, index) => existing.has(key) && recent.indexOf(key) === index);
}
