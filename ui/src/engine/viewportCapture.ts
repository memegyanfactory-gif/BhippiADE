/** Pure canvas-copy seam for ENG-186; browser-independent tests prove dimensions/annotation. */
export type CaptureSurface = {
  width: number;
  height: number;
  getContext: (kind: "2d") => {
    drawImage: (source: unknown, x: number, y: number) => void;
  } | null;
  toDataURL: (kind: "image/png") => string;
};

export function captureViewportCanvas(
  source: { width: number; height: number },
  createSurface: () => CaptureSurface,
  annotate?: (surface: CaptureSurface) => void,
): { imageBase64: string; width: number; height: number } {
  if (source.width <= 0 || source.height <= 0) {
    throw new Error("The active viewport has no drawable pixels.");
  }
  const surface = createSurface();
  surface.width = source.width;
  surface.height = source.height;
  const context = surface.getContext("2d");
  if (!context) throw new Error("The browser did not provide a 2D capture surface.");
  context.drawImage(source, 0, 0);
  annotate?.(surface);
  const dataUrl = surface.toDataURL("image/png");
  const prefix = "data:image/png;base64,";
  if (!dataUrl.startsWith(prefix)) throw new Error("The capture surface did not encode PNG.");
  return {
    imageBase64: dataUrl.slice(prefix.length),
    width: surface.width,
    height: surface.height,
  };
}
