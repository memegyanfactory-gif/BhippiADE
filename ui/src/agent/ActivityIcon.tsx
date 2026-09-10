/**
 * One small monochrome glyph per activity kind (owner spec §26, ADR-0049).
 *
 * Every icon inherits the foreground colour. Nothing here is coloured by kind: a stream of
 * twenty rows in twenty colours is a decoration, and the only thing that earns emphasis on
 * this surface is the row that is happening right now.
 */

import type { ActivityKind } from "../lib/ipc";
import {
  IconAlert,
  IconBadgeCheck,
  IconBrowser,
  IconCamera,
  IconCheck,
  IconCode,
  IconDownload,
  IconEdit,
  IconFile,
  IconFolder,
  IconGear,
  IconGitBranch,
  IconGitMerge,
  IconImage,
  IconMonitor,
  IconPlan,
  IconPlus,
  IconSearch,
  IconShield,
  IconShieldCheck,
  IconSliders,
  IconSparkle,
  IconSwap,
  IconTerminal,
  IconTrash,
} from "../components/icons";

type Glyph = typeof IconFile;

const ICONS: Partial<Record<ActivityKind, Glyph>> = {
  reasoning: IconSparkle,
  planning: IconPlan,

  searching_code: IconSearch,
  searching_files: IconSearch,
  listing_directory: IconFolder,

  reading_file: IconFile,
  reading_multiple_files: IconFile,

  searching_web: IconSearch,
  opening_webpage: IconBrowser,
  reading_webpage: IconBrowser,

  viewing_image: IconImage,
  inspecting_screenshot: IconImage,

  editing_file: IconEdit,
  creating_file: IconPlus,
  deleting_file: IconTrash,
  moving_file: IconSwap,
  applying_patch: IconEdit,

  running_command: IconTerminal,
  running_script: IconTerminal,

  starting_dev_server: IconMonitor,
  building_project: IconCode,
  installing_dependencies: IconDownload,

  running_tests: IconBadgeCheck,
  running_single_test: IconBadgeCheck,
  linting: IconSliders,
  typechecking: IconShield,

  checking_errors: IconAlert,
  debugging: IconAlert,
  investigating_failure: IconAlert,

  opening_browser: IconBrowser,
  testing_browser: IconBrowser,
  clicking_ui: IconMonitor,
  taking_screenshot: IconCamera,
  inspecting_ui: IconMonitor,

  using_tool: IconGear,
  using_plugin: IconGear,
  using_mcp: IconGear,

  starting_subagent: IconGitBranch,
  subagent_working: IconGitBranch,
  waiting_for_subagent: IconGitBranch,
  subagent_completed: IconGitBranch,

  reviewing_changes: IconGitMerge,
  reviewing_diff: IconGitMerge,

  git_status: IconGitBranch,
  git_diff: IconGitMerge,
  git_commit: IconGitBranch,

  requesting_permission: IconShield,
  waiting_for_user: IconShield,

  verifying: IconShieldCheck,
  finalizing: IconCheck,

  completed: IconCheck,
  failed: IconAlert,
};

export function ActivityIcon({ kind, size = 12 }: { kind: ActivityKind; size?: number }) {
  const Glyph = ICONS[kind] ?? IconGear;
  return <Glyph size={size} />;
}
