import { useEffect, useRef, useState } from "react";
import { bytes as formatBytes } from "../lib/format";

const IMAGE_EXTS = new Set(["png", "jpg", "jpeg", "gif", "webp", "ico", "bmp", "tiff", "tif"]);

export function isImageFile(language: string): boolean {
  return IMAGE_EXTS.has(language) || language === "svg";
}

function mimeForExt(ext: string): string {
  switch (ext) {
    case "svg": return "image/svg+xml";
    case "png": return "image/png";
    case "jpg":
    case "jpeg": return "image/jpeg";
    case "gif": return "image/gif";
    case "webp": return "image/webp";
    case "ico": return "image/x-icon";
    case "bmp": return "image/bmp";
    case "tiff":
    case "tif": return "image/tiff";
    default: return "application/octet-stream";
  }
}

export function ImageViewer({
  contentBase64,
  name,
  bytes,
  language,
}: {
  contentBase64: string;
  name: string;
  bytes: number;
  language: string;
}) {
  const [dims, setDims] = useState<{ w: number; h: number } | null>(null);
  const imgRef = useRef<HTMLImageElement | null>(null);

  const mime = mimeForExt(language);
  const src = `data:${mime};base64,${contentBase64}`;
  const isSvg = language === "svg";

  useEffect(() => {
    if (isSvg) return;
    const img = imgRef.current;
    if (!img) return;
    const onLoad = () => setDims({ w: img.naturalWidth, h: img.naturalHeight });
    img.addEventListener("load", onLoad);
    return () => img.removeEventListener("load", onLoad);
  }, [src, isSvg]);

  return (
    <div className="image-viewer">
      <div className="image-checkerboard">
        {isSvg ? (
          <div
            className="image-svg"
            dangerouslySetInnerHTML={{
              __html: atob(contentBase64),
            }}
          />
        ) : (
          <img ref={imgRef} className="image-viewer-img" src={src} alt={name} draggable={false} />
        )}
      </div>
      <div className="image-viewer-bar">
        <span className="image-viewer-name">{name}</span>
        <span className="image-viewer-meta">
          {formatBytes(bytes)}
          {dims ? ` · ${dims.w}×${dims.h}` : ""}
        </span>
      </div>
    </div>
  );
}
