// The Assets screen (docs/16 §4.2, GAD-118): the open game's asset library,
// provenance filtering, and licence editing.
//
// Walks `assets/**` with `listWorkspaceDir`, reads `<file>.meta.json` sidecars,
// supports filtering by provenance (procedural, bundled, external, user),
// and provides a "Set Licence" dialog to write/update sidecars.

import { useCallback, useEffect, useMemo, useState } from "react";
import type { ProjectSummary } from "../lib/ipc";
import { api } from "../lib/api";
import { IconSearch } from "../components/icons";
import { AssetLibraryPanel } from "../components/AssetLibraryPanel";
import {
  ASSET_KIND_LABEL,
  assetFolder,
  assetKind,
  formatBytes,
  licenceFromMeta,
  metaPathFor,
  provenanceFromMeta,
  type AssetProvenance,
} from "../lib/assetKinds";

type AssetRow = {
  path: string;
  name: string;
  size: number;
  licence: string | null;
  provenance: AssetProvenance;
  rawMeta: string | null;
};

/** How deep the walk goes. */
const MAX_DEPTH = 6;

export function Assets({ project }: { project: ProjectSummary | null }) {
  const [rows, setRows] = useState<AssetRow[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [provenanceFilter, setProvenanceFilter] = useState<AssetProvenance>("all");

  // "Set Licence" modal state
  const [editingAsset, setEditingAsset] = useState<AssetRow | null>(null);
  const [editLicence, setEditLicence] = useState("");
  const [editAuthor, setEditAuthor] = useState("");
  const [isSaving, setIsSaving] = useState(false);

  const load = useCallback(async () => {
    setRows(null);
    setError(null);
    try {
      const found: { path: string; name: string; size: number }[] = [];
      const metaText = new Map<string, string>();
      const walk = async (relative: string, depth: number) => {
        if (depth > MAX_DEPTH) return;
        const entries = await api.workspaceDir(relative);
        for (const entry of entries) {
          if (entry.is_directory) {
            await walk(entry.path, depth + 1);
          } else if (entry.name.endsWith(".meta.json")) {
            try {
              const file = await api.readFile(entry.path);
              metaText.set(entry.path, file.text);
            } catch {
              // An unreadable sidecar is the same as no sidecar: licence stays unknown.
            }
          } else {
            found.push({ path: entry.path, name: entry.name, size: entry.size });
          }
        }
      };
      await walk("assets", 0);
      setRows(
        found.map((entry) => {
          const raw = metaText.get(metaPathFor(entry.path)) ?? null;
          return {
            ...entry,
            licence: licenceFromMeta(raw),
            provenance: provenanceFromMeta(raw),
            rawMeta: raw,
          };
        }),
      );
    } catch (loadError) {
      const value = loadError as { message?: string };
      const message = value.message ?? String(loadError);
      if (/not found|no such file|cannot find/i.test(message)) {
        setRows([]);
        setError(null);
        return;
      }
      setRows(null);
      setError(message);
    }
  }, []);

  useEffect(() => {
    if (!project) {
      setRows([]);
      setError(null);
      return;
    }
    void load();
  }, [project, load]);

  const openSetLicence = (row: AssetRow) => {
    setEditingAsset(row);
    setEditLicence(row.licence ?? "project");
    let author = "";
    if (row.rawMeta) {
      try {
        const parsed = JSON.parse(row.rawMeta) as Record<string, unknown>;
        if (typeof parsed.author === "string") author = parsed.author;
      } catch {
        // ignore
      }
    }
    setEditAuthor(author);
  };

  const handleSaveLicence = async () => {
    if (!editingAsset) return;
    setIsSaving(true);
    try {
      const metaRel = metaPathFor(editingAsset.path);
      let existingObj: Record<string, unknown> = {};
      if (editingAsset.rawMeta) {
        try {
          existingObj = JSON.parse(editingAsset.rawMeta) as Record<string, unknown>;
        } catch {
          // ignore
        }
      }

      existingObj.license = editLicence.trim();
      if (editAuthor.trim().length > 0) {
        existingObj.author = editAuthor.trim();
      }
      if (!existingObj.provenance) {
        existingObj.provenance = { source: "user_configured" };
      }
      existingObj.updated_at = new Date().toISOString();

      await api.writeFile(metaRel, JSON.stringify(existingObj, null, 2));
      setEditingAsset(null);
      await load();
    } catch (saveError) {
      setError(`Failed to save licence: ${saveError}`);
    } finally {
      setIsSaving(false);
    }
  };

  const groups = useMemo(() => {
    const needle = query.trim().toLowerCase();
    const filtered = (rows ?? []).filter((row) => {
      if (provenanceFilter !== "all" && row.provenance !== provenanceFilter) {
        return false;
      }
      if (needle.length === 0) return true;
      return (
        row.name.toLowerCase().includes(needle) ||
        row.path.toLowerCase().includes(needle) ||
        (row.licence ?? "unknown").toLowerCase().includes(needle) ||
        ASSET_KIND_LABEL[assetKind(row.path)].toLowerCase().includes(needle)
      );
    });

    const byFolder = new Map<string, AssetRow[]>();
    for (const row of filtered) {
      const folder = assetFolder(row.path);
      const list = byFolder.get(folder);
      if (list) list.push(row);
      else byFolder.set(folder, [row]);
    }
    return [...byFolder.entries()]
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([folder, files]) => ({
        folder,
        files: [...files].sort((a, b) => a.name.localeCompare(b.name)),
      }));
  }, [rows, query, provenanceFilter]);

  return (
    <div className="pane">
      <div className="pane-inner plugins-inner">
        <header className="plugins-head">
          <div>
            <h1 className="screen-title">Assets</h1>
            <p className="screen-sub">
              Everything under <code>assets/</code> in {project ? project.name : "this game"},
              with verified provenance and licence status.
            </p>
          </div>

          <div className="plugins-tools">
            <div className="plugins-search">
              <IconSearch size={13} />
              <input
                type="search"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="Filter assets..."
                aria-label="Filter assets"
                disabled={rows === null}
              />
            </div>
          </div>
        </header>

        {/* Provenance filter chips */}
        <div style={{ display: "flex", gap: "8px", marginBottom: "16px", flexWrap: "wrap" }}>
          {(
            [
              { id: "all", label: "All Assets" },
              { id: "procedural", label: "Procedural" },
              { id: "bundled", label: "Bundled CC0" },
              { id: "external", label: "External AI" },
              { id: "user", label: "User / Custom" },
            ] as const
          ).map((chip) => (
            <button
              key={chip.id}
              onClick={() => setProvenanceFilter(chip.id)}
              className={provenanceFilter === chip.id ? "btn-primary" : "btn-secondary"}
              style={{ padding: "4px 10px", fontSize: "12px", borderRadius: "14px" }}
            >
              {chip.label}
            </button>
          ))}
        </div>

        {/* SPA-103: the user's own folders, above the project's assets. What is registered
            here is exactly what the AI may draw on. */}
        <AssetLibraryPanel project={project} onImported={() => void load()} />

        <h2 className="asset-project-heading">In {project ? project.name : "this game"}</h2>

        {error ? (
          <div className="plugin-banner error" role="alert">
            <span>Assets could not load: {error}</span>
            <button onClick={() => void load()}>Retry</button>
          </div>
        ) : null}

        {rows === null && error === null ? (
          <div className="plugin-grid" aria-busy="true">
            {[0, 1, 2].map((slot) => (
              <div key={slot} className="plugin-tile skeleton" />
            ))}
          </div>
        ) : groups.length === 0 ? (
          <div className="plugin-none">
            <p>
              {!project
                ? "Open a game to see its assets."
                : query.trim().length > 0 || provenanceFilter !== "all"
                  ? "No assets match this filter."
                  : "No assets yet. Anything Bhippi generates or you import lands here."}
            </p>
            {query.trim().length > 0 || provenanceFilter !== "all" ? (
              <button
                className="btn-primary"
                onClick={() => {
                  setQuery("");
                  setProvenanceFilter("all");
                }}
              >
                Clear filter
              </button>
            ) : null}
          </div>
        ) : (
          groups.map((group) => (
            <section key={group.folder} className="asset-group" aria-label={group.folder}>
              <div className="project-eyebrow">{group.folder}</div>
              <table className="table">
                <thead>
                  <tr>
                    <th scope="col">File</th>
                    <th scope="col">Kind</th>
                    <th scope="col">Provenance</th>
                    <th scope="col">Size</th>
                    <th scope="col">Licence</th>
                    <th scope="col" style={{ textAlign: "right" }}>
                      Actions
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {group.files.map((file) => (
                    <tr key={file.path}>
                      <td title={file.path}>{file.name}</td>
                      <td>{ASSET_KIND_LABEL[assetKind(file.path)]}</td>
                      <td>
                        <span
                          style={{
                            fontSize: "11px",
                            padding: "2px 6px",
                            borderRadius: "4px",
                            background: "var(--bg-card, #20232a)",
                            color: "var(--fg-muted, #8b949e)",
                            textTransform: "capitalize",
                          }}
                        >
                          {file.provenance}
                        </span>
                      </td>
                      <td className="num">{formatBytes(file.size)}</td>
                      <td>
                        {file.licence ? (
                          <span
                            style={{
                              fontSize: "11px",
                              padding: "2px 6px",
                              borderRadius: "4px",
                              fontWeight: 600,
                              background:
                                file.licence === "project" || file.licence.startsWith("CC0")
                                  ? "rgba(46, 160, 67, 0.15)"
                                  : "rgba(56, 139, 253, 0.15)",
                              color:
                                file.licence === "project" || file.licence.startsWith("CC0")
                                  ? "#3fb950"
                                  : "#58a6ff",
                            }}
                          >
                            {file.licence}
                          </span>
                        ) : (
                          <span className="asset-licence-unknown">unknown</span>
                        )}
                      </td>
                      <td style={{ textAlign: "right" }}>
                        <button
                          className="btn-secondary"
                          style={{ padding: "2px 8px", fontSize: "11px" }}
                          onClick={() => openSetLicence(file)}
                        >
                          Set Licence
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </section>
          ))
        )}

        {/* Set Licence Modal */}
        {editingAsset && (
          <div
            style={{
              position: "fixed",
              inset: 0,
              backgroundColor: "rgba(0, 0, 0, 0.6)",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              zIndex: 1000,
            }}
            role="dialog"
            aria-modal="true"
            aria-labelledby="licence-modal-title"
          >
            <div
              style={{
                background: "var(--bg-panel, #161b22)",
                border: "1px solid var(--border-color, #30363d)",
                borderRadius: "8px",
                padding: "24px",
                width: "440px",
                maxWidth: "90vw",
                display: "flex",
                flexDirection: "column",
                gap: "16px",
              }}
            >
              <h2 id="licence-modal-title" style={{ margin: 0, fontSize: "16px" }}>
                Set Licence for {editingAsset.name}
              </h2>
              <p style={{ margin: 0, fontSize: "12px", color: "var(--fg-muted, #8b949e)" }}>
                Sidecar: <code>{metaPathFor(editingAsset.path)}</code>
              </p>

              <div>
                <label style={{ fontSize: "12px", display: "block", marginBottom: "4px" }}>
                  Licence Identifier
                </label>
                <div style={{ display: "flex", gap: "6px", marginBottom: "8px", flexWrap: "wrap" }}>
                  {["project", "CC0-1.0", "MIT", "Apache-2.0", "Proprietary"].map((preset) => (
                    <button
                      key={preset}
                      type="button"
                      onClick={() => setEditLicence(preset)}
                      className={editLicence === preset ? "btn-primary" : "btn-secondary"}
                      style={{ padding: "2px 6px", fontSize: "11px" }}
                    >
                      {preset}
                    </button>
                  ))}
                </div>
                <input
                  type="text"
                  value={editLicence}
                  onChange={(e) => setEditLicence(e.target.value)}
                  placeholder="e.g. CC0-1.0 or project"
                  style={{ width: "100%", boxSizing: "border-box", padding: "6px 8px" }}
                />
              </div>

              <div>
                <label style={{ fontSize: "12px", display: "block", marginBottom: "4px" }}>
                  Author / Copyright Holder (Optional)
                </label>
                <input
                  type="text"
                  value={editAuthor}
                  onChange={(e) => setEditAuthor(e.target.value)}
                  placeholder="e.g. Studio Team or Kenney"
                  style={{ width: "100%", boxSizing: "border-box", padding: "6px 8px" }}
                />
              </div>

              <div style={{ display: "flex", justifyContent: "flex-end", gap: "8px", marginTop: "8px" }}>
                <button
                  type="button"
                  className="btn-secondary"
                  onClick={() => setEditingAsset(null)}
                  disabled={isSaving}
                >
                  Cancel
                </button>
                <button
                  type="button"
                  className="btn-primary"
                  onClick={() => void handleSaveLicence()}
                  disabled={isSaving || editLicence.trim().length === 0}
                >
                  {isSaving ? "Saving..." : "Save Licence"}
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
