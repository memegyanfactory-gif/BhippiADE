import { useCallback, useMemo, useState } from "react";
import type { SceneEntity, SceneOrganizerFolder } from "./EngineSceneDocument";
import type { EngineTemplateView } from "../lib/ipc";
import {
  IconBox,
  IconCamera,
  IconChevronDown,
  IconChevronRight,
  IconClose,
  IconLayers,
  IconEye,
  IconEyeOff,
  IconFolder,
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
  folders: SceneOrganizerFolder[];
  entityFolders: Record<string, string>;
  selection: string[];
  templates: EngineTemplateView[];
  onSelect: (id: string, additive: boolean) => void;
  onAddEntity: (template: string) => void;
  onDeleteEntity: (id: string) => void;
  onSetVisible: (id: string, visible: boolean) => void;
  onSetLocked: (id: string, locked: boolean) => void;
  onReparent: (id: string, parent: string | null) => void;
  onCreateFolder: (parent: string | null) => void;
  onRenameFolder: (folder: string, name: string) => void;
  onMoveFolder: (folder: string, parent: string | null) => void;
  onDeleteFolder: (folder: string) => void;
  onMoveEntityToFolder: (entity: string, folder: string | null) => void;
  onFocus: (id: string) => void;
}

type DragItem = { kind: "entity" | "folder"; id: string };

export function EngineHierarchy({
  entities,
  folders,
  entityFolders,
  selection,
  templates,
  onSelect,
  onAddEntity,
  onDeleteEntity,
  onSetVisible,
  onSetLocked,
  onReparent,
  onCreateFolder,
  onRenameFolder,
  onMoveFolder,
  onDeleteFolder,
  onMoveEntityToFolder,
  onFocus,
}: Props) {
  const [filter, setFilter] = useState("");
  const [kind, setKind] = useState<Filter>("all");
  const [showAddMenu, setShowAddMenu] = useState(false);
  const [showFilters, setShowFilters] = useState(false);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [dragging, setDragging] = useState<DragItem | null>(null);
  const [dropTarget, setDropTarget] = useState<string | null>(null);
  const [renamingFolder, setRenamingFolder] = useState<string | null>(null);
  const [folderDraft, setFolderDraft] = useState("");

  const childrenOf = useMemo(() => {
    const map = new Map<string | null, SceneEntity[]>();
    for (const entity of entities) {
      const key = entity.parent ?? null;
      map.set(key, [...(map.get(key) ?? []), entity]);
    }
    return map;
  }, [entities]);

  const foldersOf = useMemo(() => {
    const map = new Map<string | null, SceneOrganizerFolder[]>();
    for (const folder of folders) {
      const key = folder.parent ?? null;
      map.set(key, [...(map.get(key) ?? []), folder]);
    }
    return map;
  }, [folders]);

  // A folder assignment follows the transform tree until a descendant is explicitly
  // assigned elsewhere. That keeps attached children visually beneath their parent while
  // the persisted folder metadata remains entirely separate from `entity.parent`.
  const effectiveFolders = useMemo(() => {
    const byId = new Map(entities.map((entity) => [entity.id, entity]));
    const resolved = new Map<string, string | null>();
    const resolve = (entity: SceneEntity, active = new Set<string>()): string | null => {
      const cached = resolved.get(entity.id);
      if (cached !== undefined || resolved.has(entity.id)) return cached ?? null;
      const explicit = entityFolders[entity.id];
      if (explicit) {
        resolved.set(entity.id, explicit);
        return explicit;
      }
      if (!entity.parent || active.has(entity.id)) {
        resolved.set(entity.id, null);
        return null;
      }
      active.add(entity.id);
      const parent = byId.get(entity.parent);
      const inherited = parent ? resolve(parent, active) : null;
      resolved.set(entity.id, inherited);
      return inherited;
    };
    for (const entity of entities) resolve(entity);
    return resolved;
  }, [entities, entityFolders]);

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
  const finishFolderRename = (folder: SceneOrganizerFolder) => {
    const next = folderDraft.trim();
    if (next && next !== folder.name) onRenameFolder(folder.id, next);
    setRenamingFolder(null);
    setFolderDraft("");
  };
  const emitEntity = (entity: SceneEntity, depth: number, folderId: string | null) => {
    if (!visibleIds.has(entity.id)) return;
    const kids = (childrenOf.get(entity.id) ?? []).filter(
      (child) => visibleIds.has(child.id) && effectiveFolders.get(child.id) === folderId,
    );
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
        onDragStart={() => setDragging({ kind: "entity", id: entity.id })}
        onDragOver={(event) => {
          if (!dragging || dragging.kind !== "entity" || dragging.id === entity.id) return;
          event.preventDefault();
          setDropTarget(entity.id);
        }}
        onDragLeave={() => setDropTarget((current) => (current === entity.id ? null : current))}
        onDrop={(event) => {
          event.preventDefault();
          if (dragging?.kind === "entity" && dragging.id !== entity.id) {
            onReparent(dragging.id, entity.id);
          }
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
        <span className="outliner-row-actions">
          <button
            type="button"
            className={`outliner-toggle state${visible ? "" : " active"}`}
            title={visible ? "Hide" : "Show"}
            aria-label={visible ? `Hide ${entity.name}` : `Show ${entity.name}`}
            onClick={() => onSetVisible(entity.id, !visible)}
          >
            {visible ? <IconEye size={11} /> : <IconEyeOff size={11} />}
          </button>
          <button
            type="button"
            className={`outliner-toggle state${locked ? " active" : ""}`}
            title={locked ? "Unlock" : "Lock"}
            aria-label={locked ? `Unlock ${entity.name}` : `Lock ${entity.name}`}
            onClick={() => onSetLocked(entity.id, !locked)}
          >
            <span aria-hidden="true">{locked ? "◆" : "◇"}</span>
          </button>
          <button
            type="button"
            className="outliner-toggle destructive"
            title="Delete"
            aria-label={`Delete ${entity.name}`}
            onClick={() => onDeleteEntity(entity.id)}
          >
            <IconTrash size={11} />
          </button>
        </span>
      </div>,
    );
    if (!isCollapsed) for (const child of kids) emitEntity(child, depth + 1, folderId);
  };

  const emitFolder = (folder: SceneOrganizerFolder, depth: number) => {
    const collapseKey = `folder:${folder.id}`;
    const targetKey = `folder:${folder.id}`;
    const isCollapsed = collapsed.has(collapseKey);
    const childFolders = foldersOf.get(folder.id) ?? [];
    const entityRoots = entities.filter((entity) => {
      if (!visibleIds.has(entity.id) || effectiveFolders.get(entity.id) !== folder.id) return false;
      if (!entity.parent) return true;
      return effectiveFolders.get(entity.parent) !== folder.id;
    });
    const hasChildren = childFolders.length > 0 || entityRoots.length > 0;
    rows.push(
      <div
        key={targetKey}
        className={`outliner-row outliner-folder${dropTarget === targetKey ? " drop" : ""}`}
        style={{ paddingLeft: 4 + depth * 13 }}
        draggable={renamingFolder !== folder.id}
        onDragStart={() => setDragging({ kind: "folder", id: folder.id })}
        onDragOver={(event) => {
          if (!dragging || dragging.id === folder.id) return;
          event.preventDefault();
          setDropTarget(targetKey);
        }}
        onDragLeave={() => setDropTarget((current) => (current === targetKey ? null : current))}
        onDrop={(event) => {
          event.preventDefault();
          if (dragging?.kind === "entity") onMoveEntityToFolder(dragging.id, folder.id);
          if (dragging?.kind === "folder" && dragging.id !== folder.id) {
            onMoveFolder(dragging.id, folder.id);
          }
          setDragging(null);
          setDropTarget(null);
        }}
      >
        <button
          type="button"
          className="outliner-twisty"
          aria-label={isCollapsed ? `Expand ${folder.name}` : `Collapse ${folder.name}`}
          style={{ visibility: hasChildren ? "visible" : "hidden" }}
          onClick={() => setCollapsed((current) => {
            const next = new Set(current);
            if (next.has(collapseKey)) next.delete(collapseKey);
            else next.add(collapseKey);
            return next;
          })}
        >
          {isCollapsed ? <IconChevronRight size={10} /> : <IconChevronDown size={10} />}
        </button>
        {renamingFolder === folder.id ? (
          <input
            className="outliner-folder-name-input"
            value={folderDraft}
            aria-label={`Rename ${folder.name}`}
            autoFocus
            onChange={(event) => setFolderDraft(event.target.value)}
            onBlur={() => finishFolderRename(folder)}
            onKeyDown={(event) => {
              if (event.key === "Enter") event.currentTarget.blur();
              if (event.key === "Escape") {
                setFolderDraft("");
                setRenamingFolder(null);
              }
            }}
          />
        ) : (
          <button
            type="button"
            className="outliner-name"
            title="Organiser folder — does not affect transforms"
            onDoubleClick={() => {
              setFolderDraft(folder.name);
              setRenamingFolder(folder.id);
            }}
          >
            <IconFolder size={12} />
            <span>{folder.name}</span>
          </button>
        )}
        <span className="outliner-row-actions">
          <button type="button" className="outliner-toggle" title="Add child folder" aria-label={`Add folder inside ${folder.name}`} onClick={() => onCreateFolder(folder.id)}>
            <IconPlus size={11} />
          </button>
          <button type="button" className="outliner-toggle" title="Rename folder" aria-label={`Rename ${folder.name}`} onClick={() => { setFolderDraft(folder.name); setRenamingFolder(folder.id); }}>
            <span aria-hidden="true">Aa</span>
          </button>
          <button type="button" className="outliner-toggle destructive" title="Flatten folder (keeps every entity)" aria-label={`Flatten ${folder.name}; keep entities`} onClick={() => onDeleteFolder(folder.id)}>
            <IconTrash size={11} />
          </button>
        </span>
      </div>,
    );
    if (isCollapsed) return;
    for (const child of childFolders) emitFolder(child, depth + 1);
    for (const entity of entityRoots) emitEntity(entity, depth + 1, folder.id);
  };

  for (const folder of foldersOf.get(null) ?? []) emitFolder(folder, 0);
  const unfiledRoots = entities.filter((entity) => {
    if (!visibleIds.has(entity.id) || effectiveFolders.get(entity.id) !== null) return false;
    if (!entity.parent) return true;
    return effectiveFolders.get(entity.parent) !== null;
  });
  for (const root of unfiledRoots) emitEntity(root, 0, null);

  return (
    <aside className="engine-panel engine-hierarchy" aria-label="World Outliner">
      <div className="panel-head">
        <div className="panel-title-group">
          <span className="panel-title">Outliner</span>
          <span className="chip">{entities.length}{folders.length > 0 ? ` · ${folders.length}` : ""}</span>
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
              <button
                type="button"
                className="dropdown-item"
                onClick={() => {
                  setShowAddMenu(false);
                  onCreateFolder(null);
                }}
              >
                New Folder
              </button>
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
        <button
          type="button"
          className={`outliner-filter-button${showFilters || kind !== "all" ? " active" : ""}`}
          aria-expanded={showFilters}
          aria-controls="outliner-filters"
          onClick={() => setShowFilters((value) => !value)}
        >
          Filter{kind !== "all" ? `: ${FILTERS.find((entry) => entry.id === kind)?.label}` : ""}
        </button>
      </div>

      <div className="outliner-filters" id="outliner-filters" hidden={!showFilters}>
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
          // Folder arrangement and transform parenting remain distinct. An entity already
          // in a folder returns to the organiser root; an unfiled entity keeps the classic
          // empty-space gesture that unparents its transform.
          event.preventDefault();
          if (dragging?.kind === "folder") onMoveFolder(dragging.id, null);
          if (dragging?.kind === "entity") {
            if (entityFolders[dragging.id]) onMoveEntityToFolder(dragging.id, null);
            else onReparent(dragging.id, null);
          }
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
