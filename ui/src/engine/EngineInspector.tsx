import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { EngineAssetView, EngineComponentView, EngineFieldView } from "../lib/ipc";
import type { SceneEntity } from "./EngineSceneDocument";
import { IconChevronDown, IconClose, IconPlus, IconTrash } from "../components/icons";
import { multiFieldState } from "./multiEdit";

/**
 * The Details panel (ENG-142).
 *
 * Every control here is generated from `bhippi-engine`'s component registry, fetched over
 * IPC. That is the whole point: the panel offers exactly the fields the validator accepts,
 * so it cannot drift into showing something the engine would reject, and a component added
 * to the registry appears here with no UI change at all.
 *
 * The old panel hand-coded Transform and a handful of material paths, which is why most of
 * an entity was uneditable — nineteen components existed and two of them had a UI.
 */

/** Which accordions start open. Transform first, because it is what people reach for. */
const DEFAULT_OPEN = new Set(["Transform", "Rendering", "Physics"]);

const CATEGORY_ORDER = [
  "Transform",
  "Rendering",
  "Physics",
  "Audio",
  "Gameplay",
  "Scripting",
  "Editor",
];

interface Props {
  entity: SceneEntity | null;
  /** Full selected set, used to compute common/mixed/unavailable truth. */
  entities?: SceneEntity[];
  /** How many entities are selected — the panel says so rather than lying about one. */
  selectionCount?: number;
  /** Dispatch a component patch. The pane turns it into an engine action. */
  onPatch: (entityId: string, component: string, value: Record<string, unknown>) => void;
  onAddComponent: (entityId: string, component: string) => void;
  onRemoveComponent: (entityId: string, component: string) => void;
  onRename: (entityId: string, name: string) => void;
  onSetTags: (entityId: string, tags: string[]) => void;
}

export function EngineInspector({
  entity,
  entities = entity ? [entity] : [],
  selectionCount = entity ? 1 : 0,
  onPatch,
  onAddComponent,
  onRemoveComponent,
  onRename,
  onSetTags,
}: Props) {
  const [schema, setSchema] = useState<EngineComponentView[]>([]);
  const [assets, setAssets] = useState<EngineAssetView[]>([]);
  const [open, setOpen] = useState<Set<string>>(() => new Set(DEFAULT_OPEN));
  const [addOpen, setAddOpen] = useState(false);
  const [search, setSearch] = useState("");
  /** Field edits held until blur/Enter, so a half-typed number is never committed. */
  const [draft, setDraft] = useState<Record<string, string>>({});
  const seededFor = useRef("");

  useEffect(() => {
    void (async () => {
      const { api } = await import("../lib/api");
      try {
        setSchema(await api.engineComponentSchema());
      } catch {
        setSchema([]);
      }
      try {
        setAssets(await api.engineListAssets());
      } catch {
        setAssets([]);
      }
    })();
  }, []);

  // Re-seed the form whenever the selected entity or its payload changes, so an edit made
  // by the gizmo or by the AI shows up in the fields immediately.
  const signature = entities.map((selected) => `${selected.id}:${JSON.stringify(selected.components)}`).join("|");
  useEffect(() => {
    if (seededFor.current === signature) return;
    seededFor.current = signature;
    setDraft({});
  }, [signature]);

  const byName = useMemo(() => {
    const map = new Map<string, EngineComponentView>();
    for (const component of schema) map.set(component.name, component);
    return map;
  }, [schema]);

  const present = useMemo(() => [...new Set(
    entities.flatMap((selected) => Object.keys(selected.components ?? {})),
  )].sort(), [entities]);
  const activePresent = useMemo(
    () => (entity ? Object.keys(entity.components ?? {}) : []),
    [entity],
  );

  const grouped = useMemo(() => {
    const groups = new Map<string, string[]>();
    for (const name of present) {
      const category = byName.get(name)?.category ?? "Editor";
      groups.set(category, [...(groups.get(category) ?? []), name]);
    }
    return CATEGORY_ORDER.filter((category) => groups.has(category)).map((category) => ({
      category,
      components: groups.get(category) ?? [],
    }));
  }, [byName, present]);

  const missing = useMemo(
    () => schema.filter((component) => !activePresent.includes(component.name)),
    [activePresent, schema],
  );

  const toggle = useCallback((key: string) => {
    setOpen((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }, []);

  const commit = useCallback(
    (component: string, field: EngineFieldView, raw: string) => {
      if (!entity) return;
      if (!entities.every((selected) => selected.components[component] !== undefined)) return;
      const current = (entity.components[component] ?? {}) as Record<string, unknown>;
      const value = parseField(field, raw, current[field.name]);
      if (value === undefined) return;
      if (JSON.stringify(value) === JSON.stringify(current[field.name])) return;
      onPatch(entity.id, component, { [field.name]: value });
    },
    [entities, entity, onPatch],
  );

  if (!entity) {
    return (
      <aside className="engine-panel engine-inspector" aria-label="Details">
        <div className="panel-head">
          <span className="panel-title">Details</span>
        </div>
        <div className="engine-empty-hint">
          {selectionCount > 1
            ? `${selectionCount} entities selected. Choose the active entity to edit shared component fields.`
            : "Select an entity in the viewport or Outliner to inspect it."}
        </div>
      </aside>
    );
  }

  const key = (component: string, field: string) => `${component}.${field}`;
  const valueOf = (component: string, field: EngineFieldView): string => {
    const stored = draft[key(component, field.name)];
    if (stored !== undefined) return stored;
    if (isMixed(component, field.name)) return "";
    const payload = (entity.components[component] ?? {}) as Record<string, unknown>;
    return formatField(field, payload[field.name]);
  };
  const componentShared = (component: string) =>
    entities.every((selected) => selected.components[component] !== undefined);
  const isMixed = (component: string, field: string) =>
    multiFieldState(entities, component, field).kind === "mixed";
  const setField = (component: string, field: string, value: string) =>
    setDraft((prev) => ({ ...prev, [key(component, field)]: value }));

  const query = search.trim().toLowerCase();

  return (
    <aside className="engine-panel engine-inspector" aria-label="Details">
      <div className="panel-head">
        <span className="entity-inspector-name" title={entity.id}>
          {entity.name}
        </span>
        {selectionCount > 1 ? <span className="chip">+{selectionCount - 1}</span> : null}
      </div>

      <div className="details-identity">
        <label className="details-field">
          <span>Name</span>
          <input
            defaultValue={entity.name}
            key={`${entity.id}-name`}
            disabled={selectionCount > 1}
            title={selectionCount > 1 ? "Name is unavailable for multi-selection" : undefined}
            onBlur={(e) => {
              const next = e.target.value.trim();
              if (next && next !== entity.name) onRename(entity.id, next);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") (e.target as HTMLInputElement).blur();
            }}
          />
        </label>
        <label className="details-field">
          <span>Tags</span>
          <input
            defaultValue={entity.tags.join(", ")}
            key={`${entity.id}-tags`}
            placeholder="gameplay, prop"
            disabled={selectionCount > 1}
            title={selectionCount > 1 ? "Tags are unavailable for multi-selection" : undefined}
            onBlur={(e) => {
              const next = e.target.value
                .split(",")
                .map((tag) => tag.trim())
                .filter(Boolean);
              if (JSON.stringify(next) !== JSON.stringify(entity.tags)) onSetTags(entity.id, next);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") (e.target as HTMLInputElement).blur();
            }}
          />
        </label>
      </div>

      <div className="details-search">
        <input
          value={search}
          placeholder="Search properties…"
          aria-label="Search properties"
          onChange={(e) => setSearch(e.target.value)}
        />
        {search ? (
          <button type="button" className="hud-widget-toggle" onClick={() => setSearch("")}>
            <IconClose size={11} />
          </button>
        ) : null}
      </div>

      <div className="details-body">
        {grouped.map(({ category, components }) => {
          const visible = components.filter((component) =>
            query
              ? component.toLowerCase().includes(query) ||
                (byName.get(component)?.fields ?? []).some((field) =>
                  field.name.toLowerCase().includes(query),
                )
              : true,
          );
          if (visible.length === 0) return null;
          const isOpen = open.has(category) || query.length > 0;
          return (
            <section className="details-group" key={category}>
              <button
                type="button"
                className={`details-group-head${isOpen ? " open" : ""}`}
                onClick={() => toggle(category)}
                aria-expanded={isOpen}
              >
                <IconChevronDown size={11} />
                {category}
              </button>
              {isOpen
                ? visible.map((component) => {
                  const spec = byName.get(component);
                    const shared = componentShared(component);
                    const fields = (spec?.fields ?? []).filter((field) =>
                      query ? field.name.toLowerCase().includes(query) || component.toLowerCase().includes(query) : true,
                    );
                    return (
                      <div className="details-component" key={component}>
                        <div className="details-component-head" title={spec?.doc}>
                          <span>{component}</span>
                          {!shared ? <small>Unavailable on part of selection</small> : null}
                          {component === "Transform" ? null : (
                            <button
                              type="button"
                              className="hud-widget-toggle"
                              title={`Remove ${component}`}
                              disabled={!shared}
                              onClick={() => onRemoveComponent(entity.id, component)}
                            >
                              <IconTrash size={11} />
                            </button>
                          )}
                        </div>
                        {!spec ? (
                          <div className="engine-empty-hint">
                            Not in the component registry — shown read-only.
                          </div>
                        ) : (
                          fields.map((field) => (
                            <label className="details-field" key={field.name} title={field.doc}>
                              <span>
                                {field.name}
                                {shared && isMixed(component, field.name) ? <small>Mixed</small> : null}
                              </span>
                              <FieldInput
                                field={field}
                                assets={assets}
                                value={valueOf(component, field)}
                                mixed={shared && isMixed(component, field.name)}
                                disabled={!shared}
                                onChange={(next) => setField(component, field.name, next)}
                                onCommit={(next) => commit(component, field, next)}
                              />
                              <button
                                type="button"
                                className="details-reset"
                                disabled={!shared}
                                title={`Reset ${component}.${field.name} to its schema default`}
                                onClick={() => {
                                  setField(component, field.name, formatField(field, field.default_value));
                                  onPatch(entity.id, component, { [field.name]: field.default_value });
                                }}
                              >
                                Reset
                              </button>
                            </label>
                          ))
                        )}
                      </div>
                    );
                  })
                : null}
            </section>
          );
        })}

        <div className="details-add">
          <button
            type="button"
            className="engine-mini-btn"
            onClick={() => setAddOpen((value) => !value)}
            disabled={missing.length === 0}
          >
            <IconPlus size={12} /> Add Component
          </button>
          {addOpen ? (
            <div className="details-add-menu">
              {missing.map((component) => (
                <button
                  key={component.name}
                  type="button"
                  className="dropdown-item"
                  title={component.doc}
                  onClick={() => {
                    setAddOpen(false);
                    onAddComponent(entity.id, component.name);
                  }}
                >
                  {component.name}
                  <small>{component.category}</small>
                </button>
              ))}
            </div>
          ) : null}
        </div>
      </div>
    </aside>
  );
}

/** One control, chosen by the field's declared kind. */
function FieldInput({
  field,
  assets,
  value,
  mixed = false,
  disabled = false,
  onChange,
  onCommit,
}: {
  field: EngineFieldView;
  assets: EngineAssetView[];
  value: string;
  mixed?: boolean;
  disabled?: boolean;
  onChange: (next: string) => void;
  onCommit: (next: string) => void;
}) {
  const commitProps = {
    onBlur: (e: React.FocusEvent<HTMLInputElement | HTMLSelectElement>) => onCommit(e.target.value),
    onKeyDown: (e: React.KeyboardEvent<HTMLInputElement>) => {
      if (e.key === "Enter") (e.target as HTMLInputElement).blur();
    },
  };

  if (field.kind === "bool") {
    return (
      <input
        type="checkbox"
        checked={value === "true"}
        ref={(input) => { if (input) input.indeterminate = mixed; }}
        disabled={disabled}
        onChange={(e) => {
          const next = e.target.checked ? "true" : "false";
          onChange(next);
          onCommit(next);
        }}
      />
    );
  }
  if (field.kind === "enum") {
    return (
      <select
        value={value}
        disabled={disabled}
        onChange={(e) => {
          onChange(e.target.value);
          onCommit(e.target.value);
        }}
      >
        <option value="">{mixed ? "Mixed" : "—"}</option>
        {field.options.map((option) => (
          <option key={option} value={option}>
            {option}
          </option>
        ))}
      </select>
    );
  }
  if (field.kind === "asset") {
    const wanted = assets.filter((asset) => !field.asset_kind || asset.kind === field.asset_kind);
    return (
      <select
        value={value}
        disabled={disabled}
        onChange={(e) => {
          onChange(e.target.value);
          onCommit(e.target.value);
        }}
      >
        {/* The empty reference selects the engine's built-in, which is a real choice. */}
        <option value="">{mixed ? "Mixed" : "built-in"}</option>
        {wanted.map((asset) => (
          <option key={asset.id} value={`asset:${asset.id}`}>
            {asset.name}
          </option>
        ))}
      </select>
    );
  }
  if (field.kind === "vec3" || field.kind === "vec4") {
    const parts = value.split(",");
    const count = field.kind === "vec3" ? 3 : 4;
    return (
      <span className="details-vector">
        {Array.from({ length: count }, (_, index) => (
          <input
            key={index}
            type="number"
            step="0.1"
            value={parts[index]?.trim() ?? ""}
            placeholder={mixed ? "Mixed" : undefined}
            disabled={disabled}
            onChange={(e) => {
              const next = [...parts];
              next[index] = e.target.value;
              onChange(next.join(","));
            }}
            onBlur={(e) => {
              const next = [...parts];
              next[index] = e.target.value;
              onCommit(next.join(","));
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") (e.target as HTMLInputElement).blur();
            }}
          />
        ))}
      </span>
    );
  }
  if (field.kind === "color") {
    return (
      <input
        type="text"
        value={value}
        placeholder={mixed ? "Mixed" : "#rrggbb or r,g,b"}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value)}
        {...commitProps}
      />
    );
  }
  return (
    <input
      type={field.kind === "f32" || field.kind === "i32" ? "number" : "text"}
      step={field.kind === "f32" ? "0.1" : undefined}
      min={field.min ?? undefined}
      max={field.max ?? undefined}
      value={value}
      placeholder={mixed ? "Mixed" : undefined}
      disabled={disabled}
      onChange={(e) => onChange(e.target.value)}
      {...commitProps}
    />
  );
}

/** Render a stored payload value into the text the control shows. */
function formatField(field: EngineFieldView, value: unknown): string {
  if (value == null) {
    if (field.kind === "vec3") return "0,0,0";
    if (field.kind === "vec4") return "0,0,0,1";
    if (field.kind === "bool") return "false";
    return "";
  }
  if (Array.isArray(value)) return value.join(",");
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
}

/**
 * Turn the control's text back into the JSON the engine expects.
 *
 * `undefined` means "not a usable value yet" — a half-typed number is skipped rather than
 * committed as zero, which is the difference between a field you can type in and one that
 * fights you.
 */
function parseField(field: EngineFieldView, raw: string, current: unknown): unknown {
  const text = raw.trim();
  switch (field.kind) {
    case "bool":
      return text === "true";
    case "f32":
    case "i32": {
      if (text === "") return undefined;
      const parsed = Number(text);
      if (!Number.isFinite(parsed)) return undefined;
      return field.kind === "i32" ? Math.trunc(parsed) : parsed;
    }
    case "vec3":
    case "vec4": {
      const count = field.kind === "vec3" ? 3 : 4;
      const parts = text.split(",").map((part) => Number(part.trim()));
      if (parts.length !== count || parts.some((part) => !Number.isFinite(part))) return undefined;
      return parts;
    }
    case "color": {
      if (text.startsWith("#")) return text;
      const parts = text.split(",").map((part) => Number(part.trim()));
      if (parts.length === 3 && parts.every(Number.isFinite)) return parts;
      return undefined;
    }
    case "json": {
      if (text === "") return undefined;
      try {
        return JSON.parse(text);
      } catch {
        // Leave the current value alone rather than writing something unparseable.
        return undefined;
      }
    }
    default:
      return text === "" && current == null ? undefined : text;
  }
}
