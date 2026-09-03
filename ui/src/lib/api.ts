// Thin promise-unwrapping layer over the GENERATED bindings in ./ipc.ts (INV-032:
// this file adds no types of its own and must stay free of IPC shapes).

import { commands, events } from "./ipc";
import type {
  ComputerAction,
  EmbedSurface,
  GodotActionBatch,
  PlaytestScript,
  PresetTarget,
  ProjectAssetKind,
  ProjectTemplate,
  ProjectTool,
  TerminalShell,
  UsageWindow,
  ViewportRect,
  VisualPlaytestPlan,
} from "./ipc";

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
    attachments?: string[] | null,
  ) =>
    ok(
      commands.sendChatMessage(
        conversationId,
        text,
        providerId,
        model,
        effort,
        design,
        caveman ?? false,
        attachments ?? null,
      ),
    ),
  // The composer's `+` picked a file; Rust stats it, classifies it and — for an image
  // inside the cap — hands back the data URL the chip draws (the asset protocol is off,
  // so a `file:` src cannot load).
  attachmentPreview: (path: string) => ok(commands.attachmentPreview(path)),
  // Ctrl+V of a bitmap: Rust lands the bytes in a file and answers with the chip plus
  // the path, so the paste rides in the turn exactly like an attached file.
  savePastedImage: (dataBase64: string, mediaType: string) =>
    ok(commands.savePastedImage(dataBase64, mediaType)),
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
  // Quick / Balanced / Max: the composer's three presets over the raw pickers (GAD-017).
  tiers: () => ok(commands.getTiers()),
  setTier: (name: string, tier: { provider: string; model: string | null; effort: string }) =>
    ok(commands.setTier(name, tier)),
  stopTurn: (turnId: string) => ok(commands.stopChatTurn(turnId)),
  // CHT-115: put every file one turn changed back, and ask first whether that is possible.
  undoTurn: (turnId: string) => ok(commands.undoChatTurn(turnId)),
  turnUndoable: (turnId: string) => ok(commands.chatTurnUndoable(turnId)),
  respondPermission: (requestId: string, allow: boolean) =>
    ok(commands.respondPermission(requestId, allow)),
  usage: (window: UsageWindow | null, refreshAccounts = false) =>
    ok(commands.getUsageSummary(window, refreshAccounts)),
  setTokenCap: (providerId: string, dailyTokens: number | null) =>
    ok(commands.setProviderTokenCap(providerId, dailyTokens)),
  // SPA-003: the calendar-month dollar ceiling behind the composer's spend card.
  setMonthlySpendCap: (monthlyUsd: number | null) => ok(commands.setMonthlySpendCap(monthlyUsd)),
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
  // Blender over MCP (SPA-201): the server Bhippi attaches to a Claude Code or Codex turn.
  blenderMcpStatus: () => ok(commands.getBlenderMcpStatus()),
  setBlenderMcp: (enabled: boolean, command?: string | null, args?: string[] | null) =>
    ok(commands.setBlenderMcp(enabled, command ?? null, args ?? null)),
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
  importWorkspaceFile: (sourceAbsolute: string, destRelative: string) =>
    ok(commands.importWorkspaceFile(sourceAbsolute, destRelative)),
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
  // ── Godot (ADR-0043) ────────────────────────────────────────────────────────────
  // Every projection the pane renders — the tree, the node view, the gate findings, the
  // telemetry report — is computed in Rust. Nothing below reshapes a reply.
  godotStatus: (project: string) => ok(commands.godotStatus(project)),
  setGodotPath: (path: string, project?: string | null) =>
    ok(commands.setGodotPath(path, project ?? null)),
  checkSystemDependencies: () => ok(commands.checkSystemDependencies()),
  downloadAndInstallGodot: () => ok(commands.downloadAndInstallGodot()),
  godotCreateProject: (parent: string, name: string, template: ProjectTemplate) =>
    ok(commands.godotCreateProject(parent, name, template)),
  godotSceneTree: (project: string, sceneRel?: string | null) =>
    ok(commands.godotSceneTree(project, sceneRel ?? null)),
  godotNode: (project: string, sceneRel: string, path: string) =>
    ok(commands.godotNode(project, sceneRel, path)),
  godotListScenes: (project: string) => ok(commands.godotListScenes(project)),
  godotApplyBatch: (project: string, batch: GodotActionBatch, actor: "user" | "agent") =>
    ok(commands.godotApplyBatch(project, batch, actor)),
  godotUndoLast: (project: string) => ok(commands.godotUndoLast(project)),
  godotRun: (project: string) => ok(commands.godotRun(project)),
  godotStop: (project: string) => ok(commands.godotStop(project)),
  godotPlaytest: (project: string, inputs?: PlaytestScript | null, frames?: number | null) =>
    ok(commands.godotPlaytest(project, inputs ?? null, frames ?? null)),
  // Watch play (ADR-0044): a real Godot window Bhippi photographs and types into. `null` runs
  // the default plan, which lives in Rust because it is evidence, not a control.
  godotVisualPlaytest: (project: string, plan?: VisualPlaytestPlan | null) =>
    ok(commands.godotVisualPlaytest(project, plan ?? null)),
  godotExport: (project: string, target: PresetTarget) =>
    ok(commands.godotExport(project, target)),
  godotPackageExport: (project: string, target: PresetTarget) =>
    ok(commands.godotPackageExport(project, target)),
  godotRevealExport: (project: string, target: PresetTarget) =>
    ok(commands.godotRevealExport(project, target)),
  godotPublishWeb: (project: string) => ok(commands.godotPublishWeb(project)),
  godotExportTemplatesStatus: () => ok(commands.godotExportTemplatesStatus()),
  godotExportTemplateOffer: () => ok(commands.godotExportTemplateOffer()),
  godotOpenEditor: (project: string) => ok(commands.godotOpenEditor(project)),
  // The Games card: Rust reads the poster, counts the versions and decides what is
  // blocked. The card renders the reply and joins nothing.
  gameCardInfo: (project: string) => ok(commands.gameCardInfo(project)),
  godotCapturePoster: (project: string) => ok(commands.godotCapturePoster(project)),
  // The embedded viewport (ADR-0045): the editor and the game live inside Bhippi's window.
  godotEmbedOpenWorkspace: (project: string) => ok(commands.godotEmbedOpenWorkspace(project)),
  godotEmbedPlay: (project: string) => ok(commands.godotEmbedPlay(project)),
  godotEmbedStop: (surface: EmbedSurface) => ok(commands.godotEmbedStop(surface)),
  godotEmbedLayout: (rect: ViewportRect, visible: boolean) =>
    ok(commands.godotEmbedLayout(rect, visible)),
  godotEmbedState: () => ok(commands.godotEmbedState()),
  godotGates: (project: string, release: boolean) => ok(commands.godotGates(project, release)),
  godotPreviewStart: (project: string) => ok(commands.godotPreviewStart(project)),
  godotPreviewStop: (project: string) => ok(commands.godotPreviewStop(project)),
  godotOutput: (project: string) => ok(commands.godotOutput(project)),
  // Versions (GAD-022): the journal is the history, so the drawer only renders what Rust
  // projects out of it.
  godotListVersions: (project: string) => ok(commands.godotListVersions(project)),
  godotCreateVersion: (project: string, label: string) =>
    ok(commands.godotCreateVersion(project, label)),
  godotRevertTo: (project: string, versionId: string) =>
    ok(commands.godotRevertTo(project, versionId)),
  // The Studio bottom dock. Rust decides what an asset is, what kind it is and what its
  // licence says; the dock draws the rows.
  listProjectAssets: (project: string) => ok(commands.listProjectAssets(project)),
  listProjectScripts: (project: string) => ok(commands.listProjectScripts(project)),
  listCapabilities: () => ok(commands.listCapabilities()),
  // The asset library (SPA-101): folders outside any project Bhippi may read from. Rust
  // scans, classifies and copies; the page draws the folders and the rows.
  assetLibraryList: () => ok(commands.assetLibraryList()),
  assetLibraryAdd: (path: string) => ok(commands.assetLibraryAdd(path)),
  assetLibraryRemove: (path: string) => ok(commands.assetLibraryRemove(path)),
  assetLibrarySearch: (
    query?: string | null,
    kind?: ProjectAssetKind | null,
    limit?: number | null,
  ) => ok(commands.assetLibrarySearch(query ?? null, kind ?? null, limit ?? null)),
  assetLibraryImport: (project: string, source: string, dest?: string | null) =>
    ok(commands.assetLibraryImport(project, source, dest ?? null)),
  checkAppUpdate: () => ok(commands.checkAppUpdate()),
  installAppUpdate: () => ok(commands.installAppUpdate()),
};

export { events };
