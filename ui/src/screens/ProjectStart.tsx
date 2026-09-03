import { useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type { ProjectSummary, ToolAvailability } from "../lib/ipc";
import { api } from "../lib/api";
import {
  IconArrowLeft,
  IconAttach,
  IconClose,
  IconCode,
  IconExternal,
  IconFolder,
  IconGitBranch,
  IconPlus,
  IconSearch,
  IconTerminal,
} from "../components/icons";
import { clipName, clipPath } from "../lib/format";
import {
  ART_STYLES,
  GENRES,
  PERSPECTIVES,
  appendChip,
  chipChosen,
  composeFirstMessage,
  slugifyPrompt,
  templateForPrompt,
  uniqueFolderName,
} from "../lib/gameLauncher";
import { TierChips, type TierName } from "../components/TierChips";

type ProjectMode = "sources" | "local" | "clone" | "create";

/// Where the last parent folder went, so the second game does not ask again.
const PARENT_KEY = "bhippi-game-parent";

async function pickDirectory(title: string): Promise<string | null> {
  const selected = await open({ directory: true, multiple: false, title });
  return Array.isArray(selected)
    ? selected[0] ?? null
    : typeof selected === "string"
      ? selected
      : null;
}

const CHIP_ROWS: { label: string; chips: readonly string[] }[] = [
  { label: "Genre", chips: GENRES },
  { label: "Perspective", chips: PERSPECTIVES },
  { label: "Art style", chips: ART_STYLES },
];

/**
 * The "Describe your game" launcher (GAD-015, docs/16 §4.2).
 *
 * Same frame as the project gate it replaces — same card, same recent list, same tool row.
 * What changed is the ask: a folder is no longer the first decision, the game is. Creating
 * one derives its folder from the prompt and sends that prompt as the first Studio message,
 * so the plan starts from the sentence the user actually wrote.
 */
export function ProjectStart({
  projects,
  tools,
  onProject,
  onRefresh,
  onFirstMessage,
  chatOptions = [],
}: {
  projects: ProjectSummary[] | null;
  tools: ToolAvailability[];
  onProject: (project: ProjectSummary) => void;
  onRefresh: () => void;
  /** Carries the launcher's prompt into the new game's first Studio turn. */
  onFirstMessage?: (text: string) => void;
  /** Usable backends, so an unusable tier chip is disabled rather than silently swapped. */
  chatOptions?: { id: string; label: string }[];
}) {
  const [dialogOpen, setDialogOpen] = useState(false);
  const [prompt, setPrompt] = useState("");
  const [references, setReferences] = useState<string[]>([]);
  const [tier, setTier] = useState<TierName | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const promptRef = useRef<HTMLTextAreaElement | null>(null);

  const addChip = (chip: string) => {
    setPrompt((current) => appendChip(current, chip));
    promptRef.current?.focus();
  };

  const attachReference = async () => {
    setError(null);
    try {
      const picked = await open({
        multiple: true,
        title: "Choose reference images",
        filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "gif", "bmp"] }],
      });
      const list = Array.isArray(picked) ? picked : typeof picked === "string" ? [picked] : [];
      if (list.length > 0) setReferences((current) => [...new Set([...current, ...list])]);
    } catch (pickError) {
      setError(String((pickError as Error).message ?? pickError));
    }
  };

  const createGame = async () => {
    const described = prompt.trim();
    if (described.length === 0) {
      setError("Describe the game first — one sentence is enough.");
      promptRef.current?.focus();
      return;
    }
    setBusy(true);
    setError(null);
    try {
      let parent = "";
      try {
        parent = window.localStorage.getItem(PARENT_KEY) ?? "";
      } catch {
        parent = "";
      }
      if (!parent) {
        const chosen = await pickDirectory("Choose where your games live");
        if (!chosen) {
          setBusy(false);
          return;
        }
        parent = chosen;
        try {
          window.localStorage.setItem(PARENT_KEY, parent);
        } catch {
          // Not remembering the parent only costs one extra picker next time.
        }
      }
      const folder = uniqueFolderName(
        slugifyPrompt(described),
        (projects ?? []).map((project) => project.name),
      );
      // GAD-014: every new game is a scaffolded Godot project (INV-085) — the folder,
      // project.godot, main scene, player script, probe autoload and export presets are
      // written by Rust before the first Studio message is sent.
      const project = await api.godotCreateProject(parent, folder, templateForPrompt(described));
      onRefresh();
      onFirstMessage?.(composeFirstMessage(described, references));
      onProject(project);
    } catch (createError) {
      const value = createError as { message?: string; hint?: string };
      setError([value.message, value.hint].filter(Boolean).join(" — ") || String(createError));
      setBusy(false);
    }
  };

  const openFolder = async () => {
    setBusy(true);
    setError(null);
    try {
      const selected = await pickDirectory("Choose a game folder");
      if (!selected) {
        setBusy(false);
        return;
      }
      const project = await api.addProject(selected);
      onRefresh();
      onProject(project);
    } catch (openError) {
      const value = openError as { message?: string; hint?: string };
      setError([value.message, value.hint].filter(Boolean).join(" — ") || String(openError));
      setBusy(false);
    }
  };

  return (
    <div className="project-start">
      <div className="project-start-mark" aria-hidden="true">
        <span />
        <span />
        <span />
      </div>
      <h1>Describe your game</h1>
      <p>
        Say what you want to play. Bhippi turns it into a plan, builds it on Godot, plays it
        back to you, and keeps every change reversible.
      </p>

      <textarea
        ref={promptRef}
        className="game-prompt"
        value={prompt}
        onChange={(event) => setPrompt(event.target.value)}
        rows={4}
        placeholder="Describe your game — genre, perspective, art style, core mechanic, how you win"
        aria-label="Describe your game"
      />

      {CHIP_ROWS.map((row) => (
        <div className="game-chip-row" key={row.label}>
          <span className="project-eyebrow">{row.label}</span>
          <div className="game-chips" role="group" aria-label={row.label}>
            {row.chips.map((chip) => (
              <button
                key={chip}
                type="button"
                className={`game-chip${chipChosen(prompt, chip) ? " active" : ""}`}
                aria-pressed={chipChosen(prompt, chip)}
                onClick={() => addChip(chip)}
              >
                {chip}
              </button>
            ))}
          </div>
        </div>
      ))}

      <div className="game-launch-row">
        <button type="button" className="game-chip" onClick={() => void attachReference()}>
          <IconAttach size={13} /> Reference image
        </button>
        <TierChips
          chatOptions={chatOptions}
          active={tier}
          onSelect={(name) => setTier(name)}
          compact
        />
      </div>

      {references.length > 0 ? (
        <ul className="game-references" aria-label="Reference images">
          {references.map((path) => (
            <li key={path}>
              <span title={path}>{clipPath(path, 46)}</span>
              <button
                type="button"
                aria-label={`Remove ${path}`}
                onClick={() => setReferences((current) => current.filter((row) => row !== path))}
              >
                <IconClose size={11} />
              </button>
            </li>
          ))}
        </ul>
      ) : null}

      {error ? (
        <div className="project-error" role="alert">
          {error}
        </div>
      ) : null}

      <div className="game-launch-actions">
        <button className="project-primary" onClick={() => void createGame()} disabled={busy}>
          <IconPlus size={15} /> {busy ? "Working…" : "Create game"}
        </button>
        <button type="button" className="game-secondary" onClick={() => void openFolder()} disabled={busy}>
          <IconFolder size={14} /> Open a game folder
        </button>
      </div>

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
