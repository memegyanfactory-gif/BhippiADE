export function reconcileSessionOrder(
  current: readonly string[],
  incoming: readonly string[],
): string[] {
  const liveIds = new Set(incoming);
  const next = current.filter((id) => liveIds.has(id));
  const known = new Set(next);
  for (const id of incoming) {
    if (!known.has(id)) {
      next.push(id);
      known.add(id);
    }
  }
  return next.length === current.length && next.every((id, index) => id === current[index])
    ? (current as string[])
    : next;
}

export function resolveWorkspaceProvider(
  current: string | null,
  available: readonly string[],
  restored: readonly (string | null | undefined)[],
): string | null {
  const allowed = new Set(available);
  if (current && allowed.has(current)) return current;
  return restored.find((candidate): candidate is string => Boolean(candidate && allowed.has(candidate)))
    ?? available[0]
    ?? null;
}
