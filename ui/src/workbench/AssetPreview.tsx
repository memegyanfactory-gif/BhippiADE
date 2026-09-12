import { useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import type { Tab } from "./editorTabs";
import "../styles/asset-preview.css";

export function ImagePreview({ file }: { file: Tab }) {
  const [url, setUrl] = useState("");
  const [zoom, setZoom] = useState<number | null>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    setFailed(false);
    setSize({ width: 0, height: 0 });
    setZoom(null);
    // SVG stays in an image context: embedded scripts cannot execute in the editor.
    if (file.preview_mime === "image/svg+xml") {
      const objectUrl = URL.createObjectURL(new Blob([file.text], { type: "image/svg+xml" }));
      setUrl(objectUrl);
      return () => URL.revokeObjectURL(objectUrl);
    }
    setUrl(file.content_base64 ? `data:${file.preview_mime};base64,${file.content_base64}` : "");
  }, [file.id, file.text, file.content_base64, file.preview_mime]);
  return <div className="asset-preview">
    <div className="asset-toolbar">
      <button onClick={() => setZoom(null)} aria-pressed={zoom === null}>Fit</button>
      <button onClick={() => setZoom(1)} aria-pressed={zoom === 1}>100%</button>
      <button aria-label="Zoom out" onClick={() => setZoom(z => Math.max(.1, (z ?? 1) / 1.25))}>−</button>
      <button aria-label="Zoom in" onClick={() => setZoom(z => Math.min(8, (z ?? 1) * 1.25))}>+</button>
      <span>{zoom === null ? "Fit to view" : `${Math.round(zoom * 100)}%`}</span>
      <span className="grow" />
      {size.width > 0 ? <span>{size.width} × {size.height}</span> : null}
    </div>
    <div className={`image-preview-canvas${zoom === null ? " fit" : ""}`}>
      {file.truncated || !url || failed ? <div className="asset-error" role="alert">{file.truncated ? "This image is too large to preview (64 MB maximum)." : failed ? "This image could not be decoded. Check that the file is a valid image." : "Loading image…"}</div> :
        <img src={url} alt={file.name} draggable={false}
          style={zoom === null ? undefined : { width: size.width ? size.width * zoom : undefined, maxWidth: "none", maxHeight: "none" }}
          onLoad={event => setSize({ width: event.currentTarget.naturalWidth, height: event.currentTarget.naturalHeight })}
          onError={() => setFailed(true)} />}
    </div>
  </div>;
}

export function ModelPreview({ projectPath, relative, visible }: { projectPath: string; relative: string; visible: boolean }) {
  const surface = useRef<HTMLDivElement>(null);
  const visibility = useRef(visible);
  visibility.current = visible;
  const [attempt, setAttempt] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [ready, setReady] = useState(false);
  useEffect(() => {
    const id = crypto.randomUUID();
    let disposed = false;
    let frame = 0;
    let previous = "";
    let lastSent = 0;
    let layoutPending = false;
    setReady(false);
    setError(null);
    const fail = (value: unknown) => {
      const problem = value as { message?: string; hint?: string } | null;
      if (!disposed) setError([problem?.message ?? String(value), problem?.hint].filter(Boolean).join(" "));
    };
    // Keep native bounds in sync with resizing, scrolling, hidden tabs, and modal overlays.
    const layout = (now: number) => {
      if (disposed) return;
      const box = surface.current?.getBoundingClientRect();
      if (box) {
        const rect = { x: box.left, y: box.top, width: box.width, height: box.height };
        const show = visibility.current && document.visibilityState !== "hidden" && box.width > 0 && box.height > 0;
        const key = JSON.stringify([rect, show]);
        if (!layoutPending && (key !== previous || now - lastSent > 250)) {
          previous = key;
          lastSent = now;
          layoutPending = true;
          void api.assetPreviewLayout(id, rect, show).catch(fail).finally(() => { layoutPending = false; });
        }
      }
      frame = requestAnimationFrame(layout);
    };
    void api.assetPreviewOpen(id, projectPath, relative).then(() => {
      if (!disposed) setReady(true);
    }).catch(fail);
    frame = requestAnimationFrame(layout);
    const status = window.setInterval(() => {
      void api.assetPreviewStatus(id).then(value => { if (value) fail(value); }).catch(fail);
    }, 1000);
    return () => {
      disposed = true;
      cancelAnimationFrame(frame);
      window.clearInterval(status);
      void api.assetPreviewClose(id).catch(() => {});
    };
  }, [projectPath, relative, attempt]);
  return <div className="asset-preview">
    <div className="asset-toolbar"><strong>3D preview</strong><span>Drag to orbit · Shift + drag to pan · Scroll to zoom · F to fit</span></div>
    <div className="model-preview-surface" ref={surface}>
      {error ? <div className="asset-error" role="alert"><p>{error}</p><button onClick={() => setAttempt(v => v + 1)}>Retry</button></div> : !ready ? <div className="asset-loading">Preparing model and materials…</div> : null}
    </div>
  </div>;
}
