export const GAME_DEBUG_READY_EVENT = "bhippi:game-debug-ready";

export interface GameDebugReadyDetail {
  projectPath: string;
}

export function announceGameDebugReady(projectPath: string): void {
  window.dispatchEvent(
    new CustomEvent<GameDebugReadyDetail>(GAME_DEBUG_READY_EVENT, {
      detail: { projectPath },
    }),
  );
}
