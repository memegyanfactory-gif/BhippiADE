import type { SceneEntity } from "./EngineSceneDocument";

export type MultiFieldState =
  | { kind: "unavailable" }
  | { kind: "common"; value: unknown }
  | { kind: "mixed" };

/** Truth model behind the Details panel's common/mixed/unavailable states. */
export function multiFieldState(
  entities: readonly SceneEntity[],
  component: string,
  field: string,
): MultiFieldState {
  if (entities.length === 0 || entities.some((entity) => entity.components[component] === undefined)) {
    return { kind: "unavailable" };
  }
  const values = entities.map(
    (entity) => (entity.components[component] as Record<string, unknown>)[field],
  );
  const first = JSON.stringify(values[0]);
  return values.some((value) => JSON.stringify(value) !== first)
    ? { kind: "mixed" }
    : { kind: "common", value: values[0] };
}
