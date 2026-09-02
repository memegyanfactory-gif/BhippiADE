// Thin promise-unwrapping layer over the GENERATED bindings in ./ipc.ts (INV-032:
// this file adds no types of its own and must stay free of IPC shapes).

import { commands, events } from "./ipc";
import type { ComputerAction, ProjectTool, TerminalShell, UsageWindow } from "./ipc";

async function ok<T, E>(
  call: Promise<{ status: "ok"; data: T } | { status: "error"; error: E }>,
): Promise<T> {
  const result = await call;
  if (result.status === "ok") return result.data;
  throw result.error;
}

export const api = {
  status: () => ok(commands.getAppStatus()),
  rescanProviders: () => ok(commands.rescanProviders()),
  setProviderEnabled: (providerId: string, enabled: boolean) =>
    ok(commands.setProviderEnabled(providerId, enabled)),
  installProvider: (providerId: string) => ok(commands.installProvider(providerId)),
  conversations: () => ok(commands.listConversations()),
  workspaceSessions: () => ok(commands.listWorkspaceSessions()),
  newConversation: () => ok(commands.newConversation()),
  conversation: (conversationId: string) => ok(commands.getConversation(conversationId)),
  deleteConversation: (conversationId: string) => ok(commands.deleteConversation(conversationId)),
  sendMessage: (
    conversationId: string | null,
    text: string,
    providerId: string | null,
    model: string | null,
    effort: "fast" | "balanced" | "quality" | "ultra" | null,
    design: "off" | "on" | null,
    caveman?: boolean | null,
  ) => ok(commands.sendChatMessage(conversationId, text, providerId, model, effort, design, caveman ?? false)),
  regenerate: (
    conversationId: string,
    providerId: string | null,
    model: string | null,
    effort: "fast" | "balanced" | "quality" | "ultra" | null,
    design: "off" | "on" | null,
    caveman?: boolean | null,
  ) => ok(commands.regenerateLastAnswer(conversationId, providerId, model, effort, design, caveman ?? false)),
  setProviderModel: (providerId: string, model: string | null) =>
    ok(commands.setProviderModel(providerId, model)),
  setActiveProvider: (providerId: string | null) => ok(commands.setActiveProvider(providerId)),
  stopTurn: (turnId: string) => ok(commands.stopChatTurn(turnId)),
  // CHT-115: put every file one turn changed back, and ask first whether that is possible.
  undoTurn: (turnId: string) => ok(commands.undoChatTurn(turnId)),
  turnUndoable: (turnId: string) => ok(commands.chatTurnUndoable(turnId)),
  respondPermission: (requestId: string, allow: boolean) =>
    ok(commands.respondPermission(requestId, allow)),
  tierBudgets: () => ok(commands.getTierBudgets()),
  usage: (window: UsageWindow | null, refreshAccounts = false) =>
    ok(commands.getUsageSummary(window, refreshAccounts)),
  setTokenCap: (providerId: string, dailyTokens: number | null) =>
    ok(commands.setProviderTokenCap(providerId, dailyTokens)),
  clearUsage: (providerId: string | null) => ok(commands.clearUsage(providerId)),
  projects: () => ok(commands.listProjects()),
  addProject: (path: string) => ok(commands.addExistingProject(path)),
  createProject: (parent: string, name: string) => ok(commands.createProject(parent, name)),
  cloneProject: (gitUrl: string, parent: string) => ok(commands.cloneProject(gitUrl, parent)),
  selectProject: (path: string) => ok(commands.selectProject(path)),
  forgetProject: (path: string) => ok(commands.forgetProject(path)),
  projectTools: () => ok(commands.projectTools()),
  openProjectIn: (path: string, tool: ProjectTool) => ok(commands.openProjectIn(path, tool)),
  initializeGit: (path: string) => ok(commands.initializeProjectGit(path)),
  workspaceDir: (relative: string) => ok(commands.listWorkspaceDir(relative)),
  readFile: (relative: string) => ok(commands.readWorkspaceFile(relative)),
  writeFile: (relative: string, text: string) => ok(commands.writeWorkspaceFile(relative, text)),
  previewTargets: () => ok(commands.previewTargets()),
  projectRules: () => ok(commands.readProjectRules()),
  saveProjectRules: (text: string) => ok(commands.writeProjectRules(text)),
  computerUseStatus: () => ok(commands.getComputerUseStatus()),
  setComputerUseEnabled: (enabled: boolean) => ok(commands.setComputerUseEnabled(enabled)),
  setComputerUseFullAccess: (fullAccess: boolean) => ok(commands.setComputerUseFullAccess(fullAccess)),
  captureScreenPreview: () => ok(commands.captureScreenPreview()),
  executeComputerAction: (action: ComputerAction) => ok(commands.executeComputerAction(action)),
  listSkills: (workspace?: string | null) => ok(commands.listSkills(workspace ?? null)),
  setSkillEnabled: (skillId: string, enabled: boolean) =>
    ok(commands.setSkillEnabled(skillId, enabled)),
  listPlugins: () => ok(commands.listPlugins()),
  activatePlugin: (pluginId: string) => ok(commands.activatePlugin(pluginId)),
  deactivatePlugin: (pluginId: string) => ok(commands.deactivatePlugin(pluginId)),
  installPlugin: (pluginRef: string) => ok(commands.installPlugin(pluginRef)),
  uninstallPlugin: (pluginId: string) => ok(commands.uninstallPlugin(pluginId)),
  updatePlugin: (pluginId: string) => ok(commands.updatePlugin(pluginId)),
  importExternalSkills: (workspace?: string | null) =>
    ok(commands.importExternalSkills(workspace ?? null)),
  runProjectDiagnostics: (workspace?: string | null) =>
    ok(commands.runProjectDiagnostics(workspace ?? null)),
  cleanConversation: (conversationId: string) => ok(commands.cleanConversation(conversationId)),
  compactConversation: (conversationId: string) => ok(commands.compactConversation(conversationId)),
  reviewChanges: (workspace?: string | null, turnTitle?: string | null) =>
    ok(commands.getReviewChanges(workspace ?? null, turnTitle ?? null)),
  runCliCommand: (path: string, shell: string, command: string) =>
    ok(commands.runCliCommand(path, shell, command)),
  // A real PTY session. `runCliCommand` above is the one-shot batch runner and cannot
  // host an interactive program; these four are what a terminal pane uses.
  terminalOpen: (path: string, shell: TerminalShell, cols: number, rows: number) =>
    ok(commands.terminalOpen(path, shell, cols, rows)),
  terminalWrite: (id: string, data: string) => ok(commands.terminalWrite(id, data)),
  terminalResize: (id: string, cols: number, rows: number) =>
    ok(commands.terminalResize(id, cols, rows)),
  terminalClose: (id: string) => ok(commands.terminalClose(id)),
  openExternalTerminal: (path: string, shell: string, customCmd?: string | null) =>
    ok(commands.openExternalTerminal(path, shell, customCmd ?? null)),
  openExternalUrl: (url: string) => ok(commands.openExternalUrl(url)),
  engineStatus: () => ok(commands.getEngineStatus()),
  createGameManifest: (folderName: string | null, force: boolean) =>
    ok(commands.engineCreateGameManifest(folderName, force)),
  importWorkspaceFile: (sourceAbsolute: string, destRelative: string) =>
    ok(commands.importWorkspaceFile(sourceAbsolute, destRelative)),
  engineQueryScene: (scene?: string | null) =>
    ok(commands.engineQueryScene(scene ?? null)),
  // Every scene mutation goes through the engine's transaction path (INV-070). The pane
  // dispatches actions and renders the state that comes back; it never writes a scene file.
  engineApplyAction: (actionJson: string, scene?: string | null, label?: string | null) =>
    ok(commands.engineApplyAction(actionJson, scene ?? null, label ?? null)),
  engineOpenScene: (scene?: string | null) => ok(commands.engineOpenScene(scene ?? null)),
  engineReloadScene: (scene: string) => ok(commands.engineReloadScene(scene)),
  engineSceneDiff: (scene: string) => ok(commands.engineSceneDiff(scene)),
  engineRecoverScene: (scene: string) => ok(commands.engineRecoverScene(scene)),
  engineCloseScene: (scene: string, discard: boolean) =>
    ok(commands.engineCloseScene(scene, discard)),
  engineSaveScene: (scene?: string | null) => ok(commands.engineSaveScene(scene ?? null)),
  engineSaveAll: () => ok(commands.engineSaveAll()),
  engineUndo: (scene?: string | null) => ok(commands.engineUndo(scene ?? null)),
  engineRedo: (scene?: string | null) => ok(commands.engineRedo(scene ?? null)),
  engineSetSelection: (scene: string | null, selection: string[]) =>
    ok(commands.engineSetSelection(scene, selection)),
  engineHistory: (scene?: string | null, limit?: number | null) =>
    ok(commands.engineHistory(scene ?? null, limit ?? null)),
  engineWeatherPresets: () => ok(commands.engineWeatherPresets()),
  engineTemplates: () => ok(commands.engineTemplates()),
  enginePlayWorld: (scene?: string | null) => ok(commands.enginePlayWorld(scene ?? null)),
  engineApplyBatch: (label: string, actionsJson: string, scene?: string | null) =>
    ok(commands.engineApplyBatch(label, actionsJson, scene ?? null)),
  enginePermissionMode: () => ok(commands.enginePermissionMode()),
  // ENG-189: take back one journalled agent change — the whole batch, as one operation.
  engineUndoJournalled: (txnId: string) => ok(commands.engineUndoJournalled(txnId)),
  // ENG-190: the project's own capability switches, stored in Bhippi.game.toml.
  engineAgentCapabilities: () => ok(commands.engineAgentCapabilities()),
  engineSetAgentCapability: (capability: string, decision: string) =>
    ok(commands.engineSetAgentCapability(capability, decision)),
  // HUD editing (ENG-134…137). Every edit is a HudAction the engine validates; the panel
  // renders the state that comes back and computes nothing itself.
  hudOpen: (path?: string | null) => ok(commands.hudOpen(path ?? null)),
  hudApply: (actionJson: string, path?: string | null) =>
    ok(commands.hudApply(actionJson, path ?? null)),
  hudApplyMany: (actionsJson: string, label: string, path?: string | null) =>
    ok(commands.hudApplyMany(actionsJson, label, path ?? null)),
  hudUndo: (path?: string | null) => ok(commands.hudUndo(path ?? null)),
  hudRedo: (path?: string | null) => ok(commands.hudRedo(path ?? null)),
  hudSave: (path?: string | null) => ok(commands.hudSave(path ?? null)),
  hudReload: (path?: string | null) => ok(commands.hudReload(path ?? null)),
  hudSelect: (widget: string | null, path?: string | null) =>
    ok(commands.hudSelect(widget, path ?? null)),
  hudWidgetCatalog: () => ok(commands.hudWidgetCatalog()),
  // The component registry and asset list the Details panel renders from (ENG-142/143).
  engineComponentSchema: () => ok(commands.engineComponentSchema()),
  engineListAssets: () => ok(commands.engineListAssets()),
  // Meshes and materials resolved by the engine, so the viewport renders the real scene
  // instead of guessing from reference strings (ENG-160/162).
  engineRenderManifest: (scene?: string | null) =>
    ok(commands.engineRenderManifest(scene ?? null)),
  engineCheckContent: (release: boolean) => ok(commands.engineCheckContent(release)),
  engineSubmitScreenshot: (requestId: string, imageBase64: string, width: number, height: number) =>
    ok(commands.engineSubmitScreenshot(requestId, imageBase64, width, height)),
  engineSubmitPlaytest: (requestId: string, report: string) =>
    ok(commands.engineSubmitPlaytest(requestId, report)),
  engineSubmitGameTestBatch: (requestId: string, report: string) =>
    ok(commands.engineSubmitGameTestBatch(requestId, report)),
  engineRecordConsole: (level: string, channel: string, text: string) =>
    ok(commands.engineRecordConsole(level, channel, text)),
  engineRecordConsoleSource: (level: string, channel: string, text: string, file: string, line: number) =>
    ok(commands.engineRecordConsoleSource(level, channel, text, file, line)),
  engineConsoleRows: (level?: string | null, channel?: string | null, search?: string | null, offset = 0, limit = 40) =>
    ok(commands.engineConsoleRows(level ?? null, channel ?? null, search ?? null, offset, limit)),
  engineRecordPlayStats: (stats: import("./ipc").EnginePlayStats) =>
    ok(commands.engineRecordPlayStats(stats)),
  engineClearPlayStats: () => ok(commands.engineClearPlayStats()),
  setEnginePermissionMode: (mode: string) => ok(commands.setEnginePermissionMode(mode)),
  brainStatus: () => ok(commands.projectBrainStatus()),
  rebuildBrain: () => ok(commands.rebuildProjectBrain()),
  brainModuleCards: () => ok(commands.listProjectModuleCards()),
  brainModuleCard: (relPath: string) => ok(commands.getProjectModuleCard(relPath)),
  brainSearch: (query: string, limit?: number | null) =>
    ok(commands.searchProjectSymbols(query, limit ?? null)),
  worldScenes: () => ok(commands.worldBrainStatus()),
  worldSceneEntities: (sceneId: string) =>
    ok(commands.worldBrainSceneEntities(sceneId)),
  worldFindEntity: (sceneId: string, name: string) =>
    ok(commands.worldBrainFindEntity(sceneId, name)),
  worldIndexScene: (relPath: string, sourceRevision: number) =>
    ok(commands.worldBrainIndexScene(relPath, sourceRevision)),
  worldAssets: () => ok(commands.worldBrainAssets()),
  worldAssetsByKind: (kind: string) => ok(commands.worldBrainAssetsByKind(kind)),
  worldAssetUsage: (assetId: string) => ok(commands.worldBrainAssetUsage(assetId)),
  worldIndexAssets: (sourceRevision: number) =>
    ok(commands.worldBrainIndexAssets(sourceRevision)),
  worldPhysics: () => ok(commands.worldBrainPhysics()),
  worldPhysicsByScene: (sceneId: string) =>
    ok(commands.worldBrainPhysicsByScene(sceneId)),
  worldPhysicsByEntity: (entityId: string) =>
    ok(commands.worldBrainPhysicsByEntity(entityId)),
};

export { events };
