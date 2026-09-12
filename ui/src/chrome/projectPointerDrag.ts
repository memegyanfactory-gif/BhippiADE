/** Internal project moves must not depend on WebView2's native file-drop interception. */
export interface ProjectDrop { path: string; position: "before" | "after" }
export function projectDropAt(list: HTMLElement, x: number, y: number): ProjectDrop | null {
  const box = list.getBoundingClientRect();
  if (x < box.left || x > box.right || y < box.top || y > box.bottom) return null;
  const cards = [...list.querySelectorAll<HTMLElement>("[data-project-key]")];
  for (const card of cards) {
    const rect = card.getBoundingClientRect();
    if (y < rect.top + rect.height / 2) return { path: card.dataset.projectKey!, position: "before" };
  }
  const last = cards.at(-1);
  return last ? { path: last.dataset.projectKey!, position: "after" } : null;
}

export function beginProjectDrag(
  event: { button: number; pointerId: number; clientX: number; clientY: number; currentTarget: HTMLElement; target: EventTarget | null },
  path: string,
  update: (path: string | null, target: ProjectDrop | null) => void,
  commit: (path: string, target: ProjectDrop) => void,
  suppressClick: () => void,
): () => void {
  const hit = event.target as HTMLElement | null;
  const control = hit?.closest("button, a, input, [role='menu'], .proj-sessions");
  const list = event.currentTarget.closest<HTMLElement>(".proj-list");
  if (event.button !== 0 || !list || (control && !control.classList.contains("proj-head"))) return () => {};
  let dragging = false;
  const cleanup = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    window.removeEventListener("pointercancel", cancel);
    window.removeEventListener("blur", cancel);
    window.removeEventListener("keydown", key);
    document.body.classList.remove("project-reordering");
    update(null, null);
  };
  const move = (next: PointerEvent) => {
    if (next.pointerId !== event.pointerId) return;
    if (!dragging && Math.hypot(next.clientX - event.clientX, next.clientY - event.clientY) < 5) return;
    dragging = true;
    next.preventDefault();
    document.body.classList.add("project-reordering");
    const box = list.getBoundingClientRect();
    if (next.clientY < box.top + 28) list.scrollTop -= 14;
    if (next.clientY > box.bottom - 28) list.scrollTop += 14;
    update(path, projectDropAt(list, next.clientX, next.clientY));
  };
  const up = (next: PointerEvent) => {
    if (next.pointerId !== event.pointerId) return;
    const target = dragging ? projectDropAt(list, next.clientX, next.clientY) : null;
    if (dragging) suppressClick();
    cleanup();
    if (target && target.path !== path) commit(path, target);
  };
  const cancel = () => { if (dragging) suppressClick(); cleanup(); };
  const key = (next: KeyboardEvent) => { if (next.key === "Escape") cancel(); };
  window.addEventListener("pointermove", move, { passive: false });
  window.addEventListener("pointerup", up);
  window.addEventListener("pointercancel", cancel);
  window.addEventListener("blur", cancel);
  window.addEventListener("keydown", key);
  return cleanup;
}
