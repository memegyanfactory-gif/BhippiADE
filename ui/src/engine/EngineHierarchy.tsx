import { useCallback, useMemo, useState } from "react";
import type { SceneEntity } from "./EngineSceneDocument";
import type { EngineTemplateView } from "../lib/ipc";
import {
  IconBox,
  IconCamera,
  IconChevronDown,
  IconChevronRight,
  IconClose,
  IconLayers,
  IconPlus,
  IconSearch,
  IconSun,
  IconTrash,
} from "../components/icons";

/**
 * The World Outliner (ENG-141).
 *
 * A real tree, not a flat alphabetical list: parent/child is what the scene document
 * actually stores, and since ENG-146 it is what the viewport renders too, so the Outliner
 * showing it flat was actively misleading.
 *
 * Multi-select (Ctrl/Shift), per-row visibility and lock, and filter chips by type. Nothing
 * here computes scene state — visibility and lock are the `Visibility` component, written
 * through the same engine actions everything else uses.
 */

type Filter = "all" | "mesh" | "light" | "camera" | "physics" | "agent";

const FILTERS: { id: Filter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "mesh", label: "Meshes" },
  { id: "light", label: "Lights" },
  { id: "camera", label: "Cameras" },
  { id: "physics", label: "Physics" },
  { id: "agent", label: "AI-made" },
];

interface Props {
  entities: SceneEntity[];
  selection: string[];
  templates: EngineTemplateView[];
  onSelect: (id: string, additive: boolean) => void;
  onAddEntity: (template: string) => void;
  onDeleteEntity: (id: string) => void;
  onSetVisible: (id: string, visible: boolean) => void;
  onSetLocked: (id: string, locked: boolean) => void;
  onReparent: (id: string, parent: string | null) => void;
  onFocus: (id: string) => void;
}

export function EngineHierarchy({
  entities,
  selection,
  templates,
  onSelect,
  onAddEntity,
  onDeleteEntity,
  onSetVisible,
  onSetLocked,
  onReparent,
  onFocus,
}: Props) {
  const [filter, setFilter] = useState("");
  const [kind, setKind] = useState<Filter>("all");
  const [showAddMenu, setShowAddMenu] = useState(false);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [dragging, setDragging] = useState<string | null>(null);
  const [dropTarget, setDropTarget] = useState<string | null>(null);

  const childrenOf = useMemo(() => {
    const map = new Map<string | null, SceneEntity[]>();
    for (const entity of entities) {
      const key = entity.parent ?? null;
      map.set(key, [...(map.get(key) ?? []), entity]);
    }
    return map;
  }, [entities]);

  const matches = useCallback(
    (entity: SceneEntity) => {
      const query = filter.trim().toLowerCase();
      if (query) {
        const hit =
          entity.name.toLowerCase().includes(query) ||
          entity.tags.some((tag) => tag.toLowerCase().includes(query)) ||
          Object.keys(entity.components).some((name) => name.toLowerCase().includes(query));
        if (!hit) return false;
      }
      switch (kind) {
        case "mesh":
          return !!entity.components.MeshRenderer || !!entity.components.SkinnedMeshRenderer;
        case "light":
          return !!entity.components.Light;
        case "camera":
          return !!entity.components.Camera;
        case "physics":
          return (
            !!entity.components.RigidBody ||
            !!entity.components.Collider ||
            !!entity.components.CharacterController
          );
        case "agent":
          // Provenance is stamped on every spawn (ENG-127), so "what did the AI add?" is
          // a filter rather than a guess.
          return (entity.components.Provenance as { created_by?: string } | undefined)?.created_by === "agent";
        default:
          return true;
      }
    },
    [filter, kind],
  );

  /** An entity shows when it matches, or when a descendant does — so filtering keeps context. */
  const visibleIds = useMemo(() => {
    const keep = new Set<string>();
    const walk = (entity: SceneEntity): boolean => {
      const kids = childrenOf.get(entity.id) ?? [];
      let anyChild = false;
      for (const child of kids) if (walk(child)) anyChild = true;
      if (matches(entity) || anyChild) {
        keep.add(entity.id);
        return true;
      }
      return false;
    };
    for (const root of childrenOf.get(null) ?? []) walk(root);
    return keep;
  }, [childrenOf, matches]);

  const iconFor = (entity: SceneEntity) => {
    if (entity.components.Light) return <IconSun size={12} className="entity-icon light" />;
    if (entity.components.Camera) return <IconCamera size={12} className="entity-icon camera" />;
    if (entity.components.MeshRenderer) return <IconBox size={12} className="entity-icon mesh" />;
    return <IconLayers size={12} className="entity-icon node" />;
  };

  const rows: JSX.Element[] = [];
  const emit = (entity: SceneEntity, depth: number) => {
    if (!visibleIds.has(entity.id)) return;
    const kids = (childrenOf.get(entity.id) ?? []).filter((child) => visibleIds.has(child.id));
    const isCollapsed = collapsed.has(entity.id);
    const visibility = entity.components.Visibility as
      | { visible?: boolean; locked?: boolean }
      | undefined;
    // Absent means visible and unlocked — the component only exists once someone changes it.
    const visible = visibility?.visible !== false;
    const locked = visibility?.locked === true;
    const selected = selection.includes(entity.id);

    rows.push(
      <div
        key={entity.id}
        className={`outliner-row${selected ? " selected" : ""}${dropTarget === entity.id ? " drop" : ""}${locked ? " locked" : ""}`}
        style={{ paddingLeft: 4 + depth * 13 }}
        draggable={!locked}
        onDragStart={() => setDragging(entity.id)}
        onDragOver={(event) => {
          if (!dragging || dragging === entity.id) return;
          event.preventDefault();
          setDropTarget(entity.id);
        }}
        onDragLeave={() => setDropTarget((current) => (current === entity.id ? null : current))}
        onDrop={(event) => {
          event.preventDefault();
          if (dragging && dragging !== entity.id) onReparent(dragging, entity.id);
          setDragging(null);
          setDropTarget(null);
        }}
      >
        <button
          type="button"
          className="outliner-twisty"
          onClick={() =>
            setCollapsed((prev) => {
              const next = new Set(prev);
              if (next.has(entity.id)) next.delete(entity.id);
              else next.add(entity.id);
              return next;
            })
          }
          aria-label={isCollapsed ? "Expand" : "Collapse"}
          style={{ visibility: kids.length ? "visible" : "hidden" }}
        >
          {isCollapsed ? <IconChevronRight size={10} /> : <IconChevronDown size={10} />}
        </button>
        <button
          type="button"
          className="outliner-name"
          onClick={(event) => onSelect(entity.id, event.ctrlKey || event.metaKey || event.shiftKey)}
          onDoubleClick={() => onFocus(entity.id)}
          title={entity.tags.join(", ") || entity.name}
        >
          {iconFor(entity)}
          <span className={visible ? "" : "dimmed"}>{entity.name}</span>
        </button>
        <button
          type="button"
          className="outliner-toggle"
          title={visible ? "Hide" : "Show"}
          aria-label={visible ? `Hide ${entity.name}` : `Show ${entity.name}`}
          onClick={() => onSetVisible(entity.id, !visible)}
        >
          {visible ? "◉" : "○"}
        </button>
        <button
          type="button"
          className="outliner-toggle"
          title={locked ? "Unlock" : "Lock"}
          aria-label={locked ? `Unlock ${entity.name}` : `Lock ${entity.name}`}
          onClick={() => onSetLocked(entity.id, !locked)}
        >
          {locked ? "🔒" : "🔓"}
        </button>
        <button
          type="button"
          className="outliner-toggle"
          title="Delete"
          aria-label={`Delete ${entity.name}`}
          onClick={() => onDeleteEntity(entity.id)}
        >
          <IconTrash size={11} />
        </button>
      </div>,
    );
    if (!isCollapsed) for (const child of kids) emit(child, depth + 1);
  };
  for (const root of childrenOf.get(null) ?? []) emit(root, 0);

  return (
    <aside className="engine-panel engine-hierarchy" aria-label="World Outliner">
      <div className="panel-head">
        <div className="panel-title-group">
          <span className="panel-title">Outliner</span>
          <span className="chip">{entities.length}</span>
        </div>
        <div className="panel-actions">
          <button
            type="button"
            className="engine-mini-btn"
            onClick={() => setShowAddMenu(!showAddMenu)}
            title="Add an entity"
          >
            <IconPlus size={12} />
            <span>Add</span>
          </button>
          {showAddMenu ? (
            <div className="engine-dropdown-menu m-fade">
              {templates.map((template) => (
                <button
                  key={template.name}
                  type="button"
                  className="dropdown-item"
                  onClick={() => {
                    setShowAddMenu(false);
                    onAddEntity(template.name);
                  }}
                >
                  {template.label}
                </button>
              ))}
            </div>
          ) : null}
        </div>
      </div>

      <div className="outliner-search">
        <IconSearch size={11} />
        <input
          value={filter}
          placeholder="Search name, tag, component…"
          aria-label="Filter entities"
          onChange={(e) => setFilter(e.target.value)}
        />
        {filter ? (
          <button type="button" className="outliner-toggle" onClick={() => setFilter("")}>
            <IconClose size={10} />
          </button>
        ) : null}
      </div>

      <div className="outliner-filters">
        {FILTERS.map((entry) => (
          <button
            key={entry.id}
            type="button"
            className={`outliner-chip${kind === entry.id ? " active" : ""}`}
            onClick={() => setKind(entry.id)}
            aria-pressed={kind === entry.id}
          >
            {entry.label}
          </button>
        ))}
      </div>

      <div
        className="outliner-tree"
        onDragOver={(event) => {
          if (dragging) event.preventDefault();
        }}
        onDrop={(event) => {
          // Dropping on empty space un-parents, which is the only way back to the root.
          event.preventDefault();
          if (dragging) onReparent(dragging, null);
          setDragging(null);
          setDropTarget(null);
        }}
      >
        {rows.length > 0 ? (
          rows
        ) : (
          <div className="engine-empty-hint">
            {entities.length === 0 ? "This scene is empty — use Add." : "Nothing matches."}
          </div>
        )}
      </div>
    </aside>
  );
}
