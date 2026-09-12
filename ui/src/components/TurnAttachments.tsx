import { useEffect, useState } from "react";
import { api } from "../lib/api";
import type { AttachmentPreview } from "../lib/ipc";
import { IconFile, IconImage } from "./icons";

/**
 * What a sent message attached, drawn back into the transcript.
 *
 * A user turn stores an `Attached: shot.png (1.1 MB)` line because that is what the *model*
 * reads. The pane was rendering that line verbatim, so pasting a screenshot and pressing
 * enter turned the picture into a sentence about a picture — the composer showed a thumbnail
 * right up until the moment you sent it, and then it was gone.
 *
 * The turn keeps the paths, so the picture can come back. Previews are fetched on demand and
 * cached for the session: the same screenshot referenced by three turns is read once, and a
 * conversation full of them costs nothing until it is actually on screen.
 */

/** `null` records a path that could not be previewed, so a dead file is not retried forever. */
const previewCache = new Map<string, AttachmentPreview | null>();

/** The file name, for the one case where there is no preview to take it from. */
function basename(path: string): string {
  const cut = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return cut >= 0 ? path.slice(cut + 1) : path;
}

export function TurnAttachments({ paths }: { paths: string[] }) {
  const [previews, setPreviews] = useState<(AttachmentPreview | null)[]>(() =>
    paths.map((path) => previewCache.get(path) ?? null),
  );

  useEffect(() => {
    let live = true;
    void Promise.all(
      paths.map(async (path) => {
        const held = previewCache.get(path);
        if (held !== undefined) return held;
        try {
          const preview = await api.attachmentPreview(path);
          previewCache.set(path, preview);
          return preview;
        } catch {
          // A pasted image lives in the OS temp directory, so it can genuinely be gone by
          // the time an old conversation is reopened. That is a card, not an error: the
          // turn still says what was attached.
          previewCache.set(path, null);
          return null;
        }
      }),
    ).then((resolved) => {
      if (live) setPreviews(resolved);
    });
    return () => {
      live = false;
    };
    // Paths are fixed for a stored turn, so the join is a stable key: without it the array
    // is a new identity every render and the effect would refetch forever. `|` is safe as a
    // separator because no filesystem this runs on allows it in a path.
  }, [paths.join("|")]);

  if (paths.length === 0) return null;

  return (
    <div className="turn-attachments" role="list" aria-label="Attachments">
      {paths.map((path, index) => {
        const preview = previews[index] ?? null;
        const name = preview?.name ?? basename(path);
        if (preview?.data_url) {
          return (
            <TurnAttachmentImage key={path} src={preview.data_url} name={name} path={path} />
          );
        }
        return (
          <div key={path} role="listitem" className="turn-attachment-card" title={path}>
            <span className="turn-attachment-glyph" aria-hidden="true">
              {preview?.kind === "image" || !preview ? (
                <IconImage size={14} />
              ) : (
                <IconFile size={14} />
              )}
            </span>
            <span className="turn-attachment-meta">
              <span className="turn-attachment-name">{name}</span>
              {preview ? (
                <span className="turn-attachment-size">{preview.size_label}</span>
              ) : (
                <span className="turn-attachment-size">no longer on disk</span>
              )}
            </span>
          </div>
        );
      })}
    </div>
  );
}

/**
 * One image, at a size worth looking at.
 *
 * The composer's chip is 56px because it is a receipt for something you are about to send.
 * In the transcript it is the thing itself — a screenshot of a game someone is asking about
 * has to be legible — so it opens to a readable size and expands to full width on a click.
 */
function TurnAttachmentImage({
  src,
  name,
  path,
}: {
  src: string;
  name: string;
  path: string;
}) {
  const [expanded, setExpanded] = useState(false);
  return (
    <button
      type="button"
      role="listitem"
      className={`turn-attachment-image${expanded ? " expanded" : ""}`}
      onClick={() => setExpanded((on) => !on)}
      title={expanded ? `${path} — click to shrink` : `${path} — click to enlarge`}
      aria-label={`${name}, ${expanded ? "enlarged" : "click to enlarge"}`}
      aria-expanded={expanded}
    >
      <img src={src} alt={name} />
    </button>
  );
}
