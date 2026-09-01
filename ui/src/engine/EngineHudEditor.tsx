import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { HudState, HudWidgetKindView, HudWidgetView } from "../lib/ipc";
import { api } from "../lib/api";
import { IconClose, IconLayers, IconPlus, IconRefresh, IconSave, IconTrash } from "../components/icons";

/**
 * The HUD editor (ENG-134/135/136).
 *
 * This is the surface the whole HUD phase exists for: whatever the AI generated, the user
 * opens it here and changes it — the button's text, the bar's colour, where things sit.
 *
 * It computes nothing (INV-073). Widget rectangles arrive already resolved into canvas
 * pixels by `hud_action::resolve_rect`, and every edit is dispatched as a `HudAction` that
 * the engine validates; the panel only renders what comes back.
 */

/** What the properties form is editing. */
type Draft = Record<string, string>;

type CanvasGesture = {
  id: string;
  pointerId: number;
  mode: "move" | "resize";
  start: [number, number];
  offset: [number, number];
  size: [number, number];
  delta: [number, number];
};

const ANCHORS = [
  "top_left",
  "top_center",
  "top_right",
  "center_left",
  "center",
  "center_right",
  "bottom_left",
  "bottom_center",
  "bottom_right",
  "stretch",
] as const;

const CLICK_ACTIONS = [
  { value: "", label: "None" },
  { value: "pause_game", label: "Pause game" },
  { value: "resume_game", label: "Resume game" },
  { value: "stop_game", label: "Stop game" },
  { value: "quit_to_main", label: "Quit to Main" },
] as const;

interface Props {
  /** Bumped by the pane when the project or the file changes underneath us. */
  refreshToken: number;
  onNotice: (message: string) => void;
}

export function EngineHudEditor({ refreshToken, onNotice }: Props) {
  const [hud, setHud] = useState<HudState | null>(null);
  const [catalog, setCatalog] = useState<HudWidgetKindView[]>([]);
  const [addOpen, setAddOpen] = useState(false);
  const [saved, setSaved] = useState(false);
  const [draft, setDraft] = useState<Draft>({});
  const [gesture, setGesture] = useState<CanvasGesture | null>(null);
  const [draggedWidget, setDraggedWidget] = useState<string | null>(null);
  const [dropTarget, setDropTarget] = useState<string | null>(null);
  const [widgetFilter, setWidgetFilter] = useState("");

  const report = useCallback(
    (error: any, verb: string) => {
      const message = error?.message ?? String(error);
      const hint = error?.hint ? ` ${error.hint}` : "";
      onNotice(`Could not ${verb}: ${message}${hint}`);
    },
    [onNotice],
  );

  const load = useCallback(async () => {
    try {
      setHud(await api.hudOpen(null));
    } catch (error: any) {
      report(error, "open the HUD");
      setHud(null);
    }
  }, [report]);

  useEffect(() => {
    void load();
  }, [load, refreshToken]);

  useEffect(() => {
    void (async () => {
      try {
        setCatalog(await api.hudWidgetCatalog());
      } catch {
        setCatalog([]);
      }
    })();
  }, []);

  const doc = useMemo(() => {
    if (!hud?.document_json) return null;
    try {
      return JSON.parse(hud.document_json) as {
        widgets: {
          id: string;
          name: string;
          kind: string;
          props: Record<string, unknown>;
          style: Record<string, unknown>;
          bind: Record<string, string>;
          rect: { anchor: string; offset: [number, number]; size: [number, number]; pivot: [number, number] };
          on_click?: { action: string } | null;
        }[];
      };
    } catch {
      return null;
    }
  }, [hud?.document_json]);

  const selected = hud?.selection ?? null;
  const scale = hud ? 1 / Math.max(hud.reference[0] / 640, 1) : 1;
  const selectedView = hud?.widgets.find((widget) => widget.id === selected) ?? null;
  const selectedDoc = doc?.widgets.find((widget) => widget.id === selected) ?? null;
  const kindSchema = catalog.find((entry) => entry.kind === selectedView?.kind) ?? null;

  // The form is re-seeded whenever the selection or the document revision changes, so an
  // edit made from the canvas (or by the AI) is reflected in the fields immediately.
  const seededFor = useRef<string>("");
  useEffect(() => {
    const key = `${selected ?? ""}:${hud?.revision ?? 0}`;
    if (seededFor.current === key) return;
    seededFor.current = key;
    if (!selectedDoc) {
      setDraft({});
      return;
    }
    const next: Draft = { name: selectedDoc.name };
    for (const [key, value] of Object.entries(selectedDoc.props ?? {})) {
      next[`prop:${key}`] = typeof value === "string" ? value : JSON.stringify(value);
    }
    for (const [key, value] of Object.entries(selectedDoc.style ?? {})) {
      if (value == null) continue;
      next[`style:${key}`] = typeof value === "string" ? value : JSON.stringify(value);
    }
    for (const [key, value] of Object.entries(selectedDoc.bind ?? {})) {
      next[`bind:${key}`] = value;
    }
    next.anchor = selectedDoc.rect.anchor;
    next["offset:x"] = String(selectedDoc.rect.offset[0]);
    next["offset:y"] = String(selectedDoc.rect.offset[1]);
    next["size:w"] = String(selectedDoc.rect.size[0]);
    next["size:h"] = String(selectedDoc.rect.size[1]);
    next.on_click = selectedDoc.on_click?.action ?? "";
    setDraft(next);
  }, [selected, hud?.revision, selectedDoc]);

  const apply = useCallback(
    async (action: Record<string, unknown>) => {
      try {
        setHud(await api.hudApply(JSON.stringify(action), null));
      } catch (error: any) {
        report(error, "apply that HUD change");
      }
    },
    [report],
  );

  const applyMany = useCallback(
    async (actions: Record<string, unknown>[], label: string) => {
      if (actions.length === 0) return;
      try {
        setHud(await api.hudApplyMany(JSON.stringify(actions), label, null));
      } catch (error: any) {
        report(error, "apply those HUD changes");
      }
    },
    [report],
  );

  const select = useCallback(
    async (id: string | null) => {
      try {
        setHud(await api.hudSelect(id, null));
      } catch (error: any) {
        report(error, "select that widget");
      }
    },
    [report],
  );

  const save = useCallback(async () => {
    try {
      setHud(await api.hudSave(null));
      setSaved(true);
      window.setTimeout(() => setSaved(false), 2000);
    } catch (error: any) {
      report(error, "save the HUD");
    }
  }, [report]);

  /** Commit the whole properties form as one undo step. */
  const commitForm = useCallback(() => {
    if (!selectedDoc || !selectedView) return;
    const actions: Record<string, unknown>[] = [];
    const id = selectedDoc.id;

    if (draft.name && draft.name !== selectedDoc.name) {
      actions.push({ kind: "rename_widget", id, name: draft.name });
    }

    for (const prop of kindSchema?.props ?? []) {
      const raw = draft[`prop:${prop.name}`];
      if (raw === undefined) continue;
      const current = selectedDoc.props?.[prop.name];
      let value: unknown = raw;
      if (prop.kind === "number") {
        const parsed = Number(raw);
        if (raw.trim() === "" || Number.isNaN(parsed)) continue;
        value = parsed;
      } else if (prop.kind === "bool") {
        value = raw === "true";
      }
      if (JSON.stringify(value) === JSON.stringify(current)) continue;
      actions.push({ kind: "set_prop", id, prop: prop.name, value });
    }

    const style: Record<string, unknown> = {};
    for (const key of ["bg", "fg", "fill", "border_color", "font", "align"]) {
      const raw = draft[`style:${key}`];
      if (raw === undefined) continue;
      style[key] = raw.trim() === "" ? null : raw;
    }
    for (const key of ["radius", "opacity", "font_size", "border_width"]) {
      const raw = draft[`style:${key}`];
      if (raw === undefined || raw.trim() === "") continue;
      const parsed = Number(raw);
      if (!Number.isNaN(parsed)) style[key] = parsed;
    }
    if (Object.keys(style).length > 0) {
      actions.push({ kind: "set_style", id, style });
    }

    for (const slot of Object.keys(selectedDoc.bind ?? {})) {
      const raw = draft[`bind:${slot}`];
      if (raw === undefined || raw === selectedDoc.bind[slot]) continue;
      actions.push({ kind: "set_bind", id, slot, path: raw });
    }

    const offset: [number, number] = [Number(draft["offset:x"]), Number(draft["offset:y"])];
    const size: [number, number] = [Number(draft["size:w"]), Number(draft["size:h"])];
    const rectChanged =
      draft.anchor !== selectedDoc.rect.anchor ||
      offset[0] !== selectedDoc.rect.offset[0] ||
      offset[1] !== selectedDoc.rect.offset[1] ||
      size[0] !== selectedDoc.rect.size[0] ||
      size[1] !== selectedDoc.rect.size[1];
    if (rectChanged && offset.every(Number.isFinite) && size.every(Number.isFinite)) {
      actions.push({ kind: "set_rect", id, anchor: draft.anchor, offset, size });
    }

    const currentAction = selectedDoc.on_click?.action ?? "";
    if (draft.on_click !== undefined && draft.on_click !== currentAction) {
      actions.push({
        kind: "set_action",
        id,
        on_click: draft.on_click ? { action: draft.on_click } : null,
      });
    }

    void applyMany(actions, `edit ${selectedDoc.name}`);
  }, [applyMany, draft, kindSchema, selectedDoc, selectedView]);

  const beginCanvasGesture = useCallback((
    event: React.PointerEvent<HTMLElement>,
    widget: HudWidgetView,
    mode: CanvasGesture["mode"],
  ) => {
    if (widget.locked || !doc) return;
    const source = doc.widgets.find((entry) => entry.id === widget.id);
    if (!source) return;
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    void select(widget.id);
    setGesture({
      id: widget.id,
      pointerId: event.pointerId,
      mode,
      start: [event.clientX, event.clientY],
      offset: [...source.rect.offset],
      size: [...source.rect.size],
      delta: [0, 0],
    });
  }, [doc, select]);

  const moveCanvasGesture = useCallback((event: React.PointerEvent<HTMLElement>) => {
    setGesture((current) => {
      if (!current || current.pointerId !== event.pointerId) return current;
      // Eight reference pixels is the HUD canvas grid. Pointer motion is converted from
      // the scaled preview, then snapped; the engine still validates the final rectangle.
      const dx = Math.round(((event.clientX - current.start[0]) / scale) / 8) * 8;
      const dy = Math.round(((event.clientY - current.start[1]) / scale) / 8) * 8;
      return { ...current, delta: [dx, dy] };
    });
  }, [scale]);

  const finishCanvasGesture = useCallback((event: React.PointerEvent<HTMLElement>) => {
    if (!gesture || gesture.pointerId !== event.pointerId) return;
    event.preventDefault();
    event.stopPropagation();
    const source = doc?.widgets.find((entry) => entry.id === gesture.id);
    if (source) {
      const offset: [number, number] = gesture.mode === "move"
        ? [gesture.offset[0] + gesture.delta[0], gesture.offset[1] + gesture.delta[1]]
        : gesture.offset;
      const size: [number, number] = gesture.mode === "resize"
        ? [Math.max(8, gesture.size[0] + gesture.delta[0]), Math.max(8, gesture.size[1] + gesture.delta[1])]
        : gesture.size;
      void apply({ kind: "set_rect", id: gesture.id, anchor: source.rect.anchor, offset, size });
    }
    setGesture(null);
  }, [apply, doc, gesture]);

  if (!hud) {
    return (
      <div className="hud-editor empty">
        <IconLayers size={18} />
        <p>No HUD in this project yet.</p>
      </div>
    );
  }

  const field = (key: string, value: string) => setDraft((prev) => ({ ...prev, [key]: value }));
  const visibleWidgets = hud.widgets.filter((widget) => {
    const query = widgetFilter.trim().toLowerCase();
    return !query || widget.name.toLowerCase().includes(query) || widget.kind.includes(query);
  });
  const refocusWidget = (id: string) => requestAnimationFrame(() => {
    document.querySelector<HTMLButtonElement>(`[data-hud-widget="${CSS.escape(id)}"] .hud-widget-name`)?.focus();
  });

  return (
    <div className="hud-editor">
      <header className="hud-editor-bar">
        <span className="hud-editor-title">{hud.name}</span>
        <span className="hud-editor-path" title={hud.path}>
          {hud.path}
        </span>
        <div className="hud-editor-actions">
          <div className="spawn-entity-wrap">
            <button type="button" className="engine-mini-btn" onClick={() => setAddOpen((open) => !open)}>
              <IconPlus size={12} /> Add
            </button>
            {addOpen ? (
              <div className="engine-dropdown-menu m-fade">
                {catalog.map((entry) => (
                  <button
                    key={entry.kind}
                    type="button"
                    className="dropdown-item"
                    onClick={() => {
                      setAddOpen(false);
                      void apply({ kind: "add_widget", widget: entry.kind });
                    }}
                  >
                    {entry.label}
                  </button>
                ))}
              </div>
            ) : null}
          </div>
          <button
            type="button"
            className="engine-mini-btn"
            onClick={() => void api.hudUndo(null).then(setHud).catch((e: any) => report(e, "undo"))}
            disabled={!hud.can_undo}
            title={hud.undo_label ? `Undo ${hud.undo_label}` : "Undo"}
          >
            Undo
          </button>
          <button
            type="button"
            className="engine-mini-btn"
            onClick={() => void api.hudRedo(null).then(setHud).catch((e: any) => report(e, "redo"))}
            disabled={!hud.can_redo}
          >
            Redo
          </button>
          <button type="button" className="engine-mini-btn" onClick={() => void load()} title="Reload from disk">
            <IconRefresh size={12} />
          </button>
          <button
            type="button"
            className={`engine-save-pill-btn${saved ? " saved" : ""}`}
            onClick={() => void save()}
            title="Save the HUD"
          >
            <IconSave size={12} />
            <span>{saved ? "Saved!" : hud.dirty ? "Save *" : "Saved"}</span>
          </button>
        </div>
      </header>

      <div className="hud-editor-body">
        {/* Widget tree */}
        <aside className="hud-outliner" aria-label="HUD widgets">
          <div className="panel-head">
            <span className="panel-title">Widgets</span>
            <span className="chip">{hud.widgets.length}</span>
          </div>
          <label className="hud-widget-filter">
            <span className="sr-only">Filter HUD widgets</span>
            <input
              value={widgetFilter}
              placeholder="Filter widgets…"
              onChange={(event) => setWidgetFilter(event.target.value)}
            />
          </label>
          <div
            className="hud-widget-list"
            onDragOver={(event) => event.preventDefault()}
            onDrop={(event) => {
              if (event.target !== event.currentTarget || !draggedWidget) return;
              event.preventDefault();
              void apply({ kind: "reparent_widget", id: draggedWidget, parent: null });
              setDraggedWidget(null);
              setDropTarget(null);
            }}
          >
            {visibleWidgets.map((widget, widgetIndex) => (
              <div
                key={widget.id}
                data-hud-widget={widget.id}
                className={`hud-widget-row${widget.id === selected ? " selected" : ""}${widget.id === draggedWidget ? " dragging" : ""}${widget.id === dropTarget ? " drop-target" : ""}`}
                style={{ paddingLeft: 8 + widget.depth * 14 }}
                draggable={!widget.locked}
                onDragStart={(event) => {
                  setDraggedWidget(widget.id);
                  event.dataTransfer.effectAllowed = "move";
                  event.dataTransfer.setData("text/bhippi-hud-widget", widget.id);
                }}
                onDragEnd={() => setDraggedWidget(null)}
                onDragOver={(event) => {
                  if (draggedWidget && draggedWidget !== widget.id) {
                    event.preventDefault();
                    setDropTarget(widget.id);
                  }
                }}
                onDragLeave={() => setDropTarget((current) => current === widget.id ? null : current)}
                onDrop={(event) => {
                  event.preventDefault();
                  event.stopPropagation();
                  if (!draggedWidget || draggedWidget === widget.id) return;
                  const parent = widget.is_container ? widget.id : widget.parent;
                  const order = widget.is_container
                    ? Math.max(0, ...hud.widgets.filter((entry) => entry.parent === widget.id).map((entry) => entry.order + 1))
                    : widget.order;
                  void applyMany([
                    { kind: "reparent_widget", id: draggedWidget, parent },
                    { kind: "reorder_widget", id: draggedWidget, order },
                  ], `move ${hud.widgets.find((entry) => entry.id === draggedWidget)?.name ?? "widget"}`);
                  setDraggedWidget(null);
                  setDropTarget(null);
                }}
              >
                <button
                  type="button"
                  className="hud-widget-name"
                  onClick={() => void select(widget.id)}
                  onKeyDown={(event) => {
                    if (!event.altKey || widget.locked) return;
                    const action = event.key;
                    if (!["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(action)) return;
                    event.preventDefault();
                    if (action === "ArrowUp" || action === "ArrowDown") {
                      const order = widget.order + (action === "ArrowUp" ? -1 : 1);
                      void applyMany(
                        [{ kind: "reorder_widget", id: widget.id, order }],
                        `reorder ${widget.name}`,
                      ).then(() => refocusWidget(widget.id));
                      return;
                    }
                    if (action === "ArrowLeft") {
                      if (!widget.parent) return;
                      const currentParent = hud.widgets.find((entry) => entry.id === widget.parent);
                      void applyMany(
                        [{ kind: "reparent_widget", id: widget.id, parent: currentParent?.parent ?? null }],
                        `outdent ${widget.name}`,
                      ).then(() => refocusWidget(widget.id));
                      return;
                    }
                    const container = visibleWidgets
                      .slice(0, widgetIndex)
                      .reverse()
                      .find((entry) => entry.is_container && entry.id !== widget.id);
                    if (!container) return;
                    const order = Math.max(
                      0,
                      ...hud.widgets
                        .filter((entry) => entry.parent === container.id)
                        .map((entry) => entry.order + 1),
                    );
                    void applyMany(
                      [
                        { kind: "reparent_widget", id: widget.id, parent: container.id },
                        { kind: "reorder_widget", id: widget.id, order },
                      ],
                      `indent ${widget.name}`,
                    ).then(() => refocusWidget(widget.id));
                  }}
                >
                  <span className="hud-widget-kind">{widget.kind}</span>
                  {widget.name}
                </button>
                <button
                  type="button"
                  className="hud-widget-toggle"
                  title={widget.visible ? "Hide" : "Show"}
                  onClick={() => void apply({ kind: "set_visible", id: widget.id, visible: !widget.visible })}
                >
                  {widget.visible ? "◉" : "○"}
                </button>
                <button
                  type="button"
                  className="hud-widget-toggle"
                  title={widget.locked ? "Unlock" : "Lock"}
                  onClick={() => void apply({ kind: "set_locked", id: widget.id, locked: !widget.locked })}
                >
                  {widget.locked ? "🔒" : "◇"}
                </button>
                <button
                  type="button"
                  className="hud-widget-toggle"
                  title="Remove"
                  onClick={() => void apply({ kind: "remove_widget", id: widget.id })}
                >
                  <IconTrash size={11} />
                </button>
              </div>
            ))}
            {hud.widgets.length === 0 ? <div className="engine-empty-hint">No widgets yet — use Add.</div> : null}
          </div>
        </aside>

        {/* Canvas preview: rectangles come pre-resolved from the engine. */}
        <main className="hud-canvas-wrap">
          <div
            className="hud-canvas"
            style={{ width: hud.reference[0] * scale, height: hud.reference[1] * scale }}
            onClick={() => void select(null)}
          >
            {hud.safe_area > 0 ? (
              <div
                className="hud-safe-area"
                style={{ inset: `${hud.safe_area * 100}% ${hud.safe_area * 100}%` }}
              />
            ) : null}
            {hud.widgets
              .filter((widget) => widget.visible)
              .map((widget) => (
                <div
                  key={widget.id}
                  role="button"
                  tabIndex={0}
                  className={`hud-canvas-widget${widget.id === selected ? " selected" : ""}`}
                  style={{
                    left: (widget.rect[0] + (gesture?.id === widget.id && gesture.mode === "move" ? gesture.delta[0] : 0)) * scale,
                    top: (widget.rect[1] + (gesture?.id === widget.id && gesture.mode === "move" ? gesture.delta[1] : 0)) * scale,
                    width: Math.max(8, widget.rect[2] + (gesture?.id === widget.id && gesture.mode === "resize" ? gesture.delta[0] : 0)) * scale,
                    height: Math.max(8, widget.rect[3] + (gesture?.id === widget.id && gesture.mode === "resize" ? gesture.delta[1] : 0)) * scale,
                  }}
                  aria-label={`${widget.name} (${widget.kind})`}
                  aria-disabled={widget.locked}
                  onPointerDown={(event) => beginCanvasGesture(event, widget, "move")}
                  onPointerMove={moveCanvasGesture}
                  onPointerUp={finishCanvasGesture}
                  onPointerCancel={() => setGesture(null)}
                  onClick={(event) => {
                    event.stopPropagation();
                    void select(widget.id);
                  }}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" || event.key === " ") {
                      event.preventDefault();
                      void select(widget.id);
                    }
                  }}
                  title={`${widget.name} (${widget.kind})`}
                >
                  <span>{labelFor(widget, doc)}</span>
                  {widget.id === selected && !widget.locked ? (
                    <span
                      className="hud-resize-handle"
                      aria-hidden="true"
                      onPointerDown={(event) => beginCanvasGesture(event, widget, "resize")}
                    />
                  ) : null}
                </div>
              ))}
          </div>
          <p className="hud-canvas-note">
            {hud.reference[0]}×{hud.reference[1]} reference · {hud.scale_mode}
          </p>
        </main>

        {/* Details */}
        <aside className="hud-details" aria-label="Widget details">
          <div className="panel-head">
            <span className="panel-title">{selectedView ? selectedView.name : "Details"}</span>
            {selectedView ? (
              <button type="button" className="hud-widget-toggle" onClick={() => void select(null)}>
                <IconClose size={11} />
              </button>
            ) : null}
          </div>
          {!selectedView || !selectedDoc ? (
            <div className="engine-empty-hint">Select a widget to edit it.</div>
          ) : (
            <div className="hud-form">
              <label className="hud-field">
                <span>Name</span>
                <input value={draft.name ?? ""} onChange={(e) => field("name", e.target.value)} />
              </label>

              {(kindSchema?.props ?? []).map((prop) => (
                <label className="hud-field" key={prop.name} title={prop.doc}>
                  <span>{prop.name}</span>
                  {prop.kind === "enum" ? (
                    <select
                      value={draft[`prop:${prop.name}`] ?? ""}
                      onChange={(e) => field(`prop:${prop.name}`, e.target.value)}
                    >
                      <option value="">—</option>
                      {prop.options.map((option) => (
                        <option key={option} value={option}>
                          {option}
                        </option>
                      ))}
                    </select>
                  ) : prop.kind === "bool" ? (
                    <select
                      value={draft[`prop:${prop.name}`] ?? "false"}
                      onChange={(e) => field(`prop:${prop.name}`, e.target.value)}
                    >
                      <option value="true">true</option>
                      <option value="false">false</option>
                    </select>
                  ) : (
                    <input
                      type={prop.kind === "number" ? "number" : "text"}
                      value={draft[`prop:${prop.name}`] ?? ""}
                      onChange={(e) => field(`prop:${prop.name}`, e.target.value)}
                    />
                  )}
                </label>
              ))}

              <div className="hud-form-section">Layout</div>
              <div className="hud-z-order-actions" role="group" aria-label="Widget draw order">
                <button type="button" onClick={() => void apply({ kind: "reorder_widget", id: selectedView.id, order: selectedView.order + 1 })}>
                  Bring Forward
                </button>
                <button type="button" onClick={() => void apply({ kind: "reorder_widget", id: selectedView.id, order: selectedView.order - 1 })}>
                  Send Back
                </button>
              </div>
              <label className="hud-field">
                <span>Anchor</span>
                <select value={draft.anchor ?? "top_left"} onChange={(e) => field("anchor", e.target.value)}>
                  {ANCHORS.map((anchor) => (
                    <option key={anchor} value={anchor}>
                      {anchor}
                    </option>
                  ))}
                </select>
              </label>
              <div className="hud-field-row">
                <label className="hud-field">
                  <span>X</span>
                  <input type="number" value={draft["offset:x"] ?? ""} onChange={(e) => field("offset:x", e.target.value)} />
                </label>
                <label className="hud-field">
                  <span>Y</span>
                  <input type="number" value={draft["offset:y"] ?? ""} onChange={(e) => field("offset:y", e.target.value)} />
                </label>
              </div>
              <div className="hud-field-row">
                <label className="hud-field">
                  <span>W</span>
                  <input type="number" value={draft["size:w"] ?? ""} onChange={(e) => field("size:w", e.target.value)} />
                </label>
                <label className="hud-field">
                  <span>H</span>
                  <input type="number" value={draft["size:h"] ?? ""} onChange={(e) => field("size:h", e.target.value)} />
                </label>
              </div>

              <div className="hud-form-section">Style</div>
              {["bg", "fg", "fill", "font_size", "radius", "opacity", "align"].map((key) => (
                <label className="hud-field" key={key}>
                  <span>{key}</span>
                  <input
                    value={draft[`style:${key}`] ?? ""}
                    placeholder="—"
                    onChange={(e) => field(`style:${key}`, e.target.value)}
                  />
                </label>
              ))}

              {Object.keys(selectedDoc.bind ?? {}).length > 0 ? (
                <>
                  <div className="hud-form-section">Bindings</div>
                  {Object.keys(selectedDoc.bind).map((slot) => (
                    <label className="hud-field" key={slot}>
                      <span>{slot}</span>
                      <input
                        value={draft[`bind:${slot}`] ?? ""}
                        onChange={(e) => field(`bind:${slot}`, e.target.value)}
                      />
                    </label>
                  ))}
                </>
              ) : null}

              {selectedView.kind === "button" ? (
                <>
                  <div className="hud-form-section">On click</div>
                  <label className="hud-field">
                    <span>Action</span>
                    <select value={draft.on_click ?? ""} onChange={(e) => field("on_click", e.target.value)}>
                      {CLICK_ACTIONS.map((action) => (
                        <option key={action.value} value={action.value}>
                          {action.label}
                        </option>
                      ))}
                    </select>
                  </label>
                </>
              ) : null}

              <button type="button" className="engine-save-pill-btn" onClick={commitForm}>
                Apply changes
              </button>
            </div>
          )}
        </aside>
      </div>
    </div>
  );
}

/** What a widget shows on the canvas: its text if it has one, else its name. */
function labelFor(widget: HudWidgetView, doc: { widgets: { id: string; props: Record<string, unknown> }[] } | null) {
  const props = doc?.widgets.find((entry) => entry.id === widget.id)?.props;
  const text = props?.text;
  return typeof text === "string" && text.length > 0 ? text : widget.name;
}
