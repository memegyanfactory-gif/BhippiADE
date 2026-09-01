export type ScenePatchPlan = {
  full: boolean;
  rebuildIds: Set<string>;
};

/** Pure decision seam for ENG-107; the renderer consumes this plan without guessing. */
export function planScenePatch(
  currentSceneId: string | null,
  nextSceneId: string | null,
  nextEntityIds: readonly string[],
  touchedIds: readonly string[] | null,
  manifestUnchanged: boolean,
): ScenePatchPlan {
  const full =
    nextSceneId === null
    || currentSceneId !== nextSceneId
    || touchedIds === null
    || !manifestUnchanged;
  return {
    full,
    rebuildIds: new Set(full ? nextEntityIds : touchedIds),
  };
}
