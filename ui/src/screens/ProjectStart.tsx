import { useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type { ProjectSummary, ToolAvailability } from "../lib/ipc";
import { api } from "../lib/api";
import {
  IconArrowLeft,
  IconCode,
  IconExternal,
  IconFolder,
  IconGitBranch,
  IconPlus,
  IconSearch,
  IconTerminal,
} from "../components/icons";
import { clipName, clipPath } from "../lib/format";

type ProjectMode = "sources" | "local" | "clone" | "create";

async function pickDirectory(title: string): Promise<string | null> {
  const selected = await open({ directory: true, multiple: false, title });
  return typeof selected === "string" ? selected : null;
}

export function ProjectStart({
  projects,
  tools,
  onProject,
  onRefresh,
}: {
  projects: ProjectSummary[] | null;
  tools: ToolAvailability[];
  onProject: (project: ProjectSummary) => void;
  onRefresh: () => void;
}) {
  const [dialogOpen, setDialogOpen] = useState(false);

  return (
    <div className="project-start">
      <div className="project-start-mark" aria-hidden="true">
        <span />
        <span />
        <span />
      </div>
      <h1>Start with a project</h1>
      <p>
        Open a folder, clone a repository, or create a clean workspace. Bhippi keeps every
        agent session attached to the project you choose.
      </p>
      <button className="project-primary" onClick={() => setDialogOpen(true)}>
        <IconPlus size={15} /> Add project
      </button>

      {projects && projects.length > 0 ? (
        <section className="recent-projects" aria-label="Recent projects">
          <div className="project-eyebrow">Recent projects</div>
          {projects.slice(0, 5).map((project) => (
            <button key={project.path} className="recent-project" onClick={() => onProject(project)}>
              <span className="recent-project-icon"><IconFolder /></span>
              <span className="recent-project-copy">
                <strong>{clipName(project.name, 28)}</strong>
                <span title={project.path}>{clipPath(project.path, 46)}</span>
              </span>
              {project.is_git_repository ? (
                <span className="recent-project-git"><IconGitBranch size={13} />{project.branch ?? "Git"}</span>
              ) : null}
              <IconExternal size={13} />
            </button>
          ))}
        </section>
      ) : null}

      <div className="tool-connect-row" aria-label="Available development tools">
        {tools.map((tool) => (
          <span key={tool.tool} className={tool.available ? "available" : ""} title={tool.hint}>
            {tool.tool === "explorer" ? <IconExternal size={13} /> : tool.tool === "antigravity" ? <IconTerminal size={13} /> : <IconCode size={13} />} {tool.label}
          </span>
        ))}
      </div>

      {dialogOpen ? (
        <ProjectDialog
          onClose={() => setDialogOpen(false)}
          onCreated={(project) => {
            setDialogOpen(false);
            onRefresh();
            onProject(project);
          }}
        />
      ) : null}
    </div>
  );
}

export function ProjectDialog({
  onClose,
  onCreated,
  initialMode,
}: {
  onClose: () => void;
  onCreated: (project: ProjectSummary) => void;
  /** Skip the source list and open straight on a flow ("create" / "clone"). */
  initialMode?: "create" | "clone";
}) {
  const [mode, setMode] = useState<ProjectMode>(initialMode ?? "sources");
  const [query, setQuery] = useState("");
  const [path, setPath] = useState("");
  const [parent, setParent] = useState("");
  const [name, setName] = useState("");
  const [gitUrl, setGitUrl] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const firstRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => firstRef.current?.focus(), [mode]);
  useEffect(() => {
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", escape);
    return () => window.removeEventListener("keydown", escape);
  }, [onClose]);

  const sources = useMemo(
    () => [
      { id: "local" as const, title: "Local folder", detail: "Open an existing folder on this computer", icon: IconFolder },
      { id: "clone" as const, title: "Git URL", detail: "Clone a repository over HTTPS or SSH", icon: IconGitBranch },
      { id: "create" as const, title: "Create project", detail: "Make a new empty project folder", icon: IconPlus },
    ].filter((source) => `${source.title} ${source.detail}`.toLowerCase().includes(query.toLowerCase())),
    [query],
  );

  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      const project =
        mode === "local"
          ? await api.addProject(path)
          : mode === "clone"
            ? await api.cloneProject(gitUrl, parent)
            : await api.createProject(parent, name);
      onCreated(project);
    } catch (submitError) {
      const value = submitError as { message?: string; hint?: string };
      setError([value.message, value.hint].filter(Boolean).join(" — ") || String(submitError));
      setBusy(false);
    }
  };

  const chooseLocalFolder = async () => {
    setBusy(true);
    setError(null);
    try {
      const selected = await pickDirectory("Choose a project folder");
      if (!selected) {
        setMode("local");
        setBusy(false);
        return;
      }
      setPath(selected);
      onCreated(await api.addProject(selected));
    } catch (pickError) {
      const value = pickError as { message?: string; hint?: string };
      setError([value.message, value.hint].filter(Boolean).join(" — ") || String(pickError));
      setMode("local");
      setBusy(false);
    }
  };

  const chooseParentFolder = async () => {
    setError(null);
    try {
      const selected = await pickDirectory("Choose a parent folder");
      if (selected) setParent(selected);
    } catch (pickError) {
      setError(String(pickError));
    }
  };

  return (
    <div className="dialog-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <section className="project-dialog" role="dialog" aria-modal="true" aria-label="Add project">
        <header>
          <button onClick={() => (mode === "sources" ? onClose() : setMode("sources"))} aria-label="Back">
            <IconArrowLeft size={15} />
          </button>
          <strong>{mode === "sources" ? "Add a project" : mode === "local" ? "Open local folder" : mode === "clone" ? "Clone repository" : "Create project"}</strong>
          <span />
        </header>

        {mode === "sources" ? (
          <>
            <label className="project-search">
              <IconSearch size={14} />
              <input ref={firstRef} value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search sources…" aria-label="Search project sources" />
            </label>
            <div className="source-label">Sources</div>
            <div className="source-list">
              {sources.map(({ id, title, detail, icon: Glyph }) => (
                <button key={id} onClick={() => id === "local" ? void chooseLocalFolder() : setMode(id)} disabled={busy}>
                  <span className="source-icon"><Glyph size={17} /></span>
                  <span><strong>{title}</strong><small>{detail}</small></span>
                  <span className="source-enter">↵</span>
                </button>
              ))}
            </div>
          </>
        ) : (
          <form className="project-form" onSubmit={(event) => { event.preventDefault(); void submit(); }}>
            {mode === "local" ? (
              <label>Folder path<span className="path-picker-field"><input ref={firstRef} value={path} onChange={(event) => setPath(event.target.value)} placeholder="C:\\Work\\my-project" /><button type="button" onClick={() => void chooseLocalFolder()}>Browse…</button></span></label>
            ) : (
              <>
                <label>Parent folder<span className="path-picker-field"><input ref={firstRef} value={parent} onChange={(event) => setParent(event.target.value)} placeholder="C:\\Work" /><button type="button" onClick={() => void chooseParentFolder()}>Browse…</button></span></label>
                {mode === "clone" ? (
                  <label>Git URL<input value={gitUrl} onChange={(event) => setGitUrl(event.target.value)} placeholder="https://github.com/owner/repository.git" /></label>
                ) : (
                  <label>Project name<input value={name} onChange={(event) => setName(event.target.value)} placeholder="my-project" /></label>
                )}
              </>
            )}
            <p className="project-form-note">Paths are validated by the desktop app. Removing a project later never deletes its files.</p>
            {error ? <div className="project-error" role="alert">{error}</div> : null}
            <div className="project-form-actions">
              <button type="button" onClick={onClose}>Cancel</button>
              <button className="project-primary" type="submit" disabled={busy}>{busy ? "Working…" : mode === "clone" ? "Clone project" : mode === "create" ? "Create project" : "Open project"}</button>
            </div>
          </form>
        )}
      </section>
    </div>
  );
}
