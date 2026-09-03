/**
 * Shapes the Studio surfaces render (GAD-020…023). Projections only — every number in
 * them is computed in Rust or typed in by the owner; the page adds nothing (INV-073).
 */

/** The manifest's `[game]` and `[publish]` tables as the Game settings modal edits them. */
export interface GameSettingsData {
  title: string;
  description: string;
  tags: string[];
  posterPath: string;
  webExportDir: string;
  includeCredits: boolean;
  windowWidth: number;
  windowHeight: number;
}

/** One planned system on a plan card, with whether the build has finished it. */
export interface GamePlanSystem {
  name: string;
  desc: string;
  done: boolean;
}

/** An open question the plan card asks before the build starts. */
export interface GamePlanQuestion {
  id: string;
  question: string;
  options: string[];
  selected?: string;
}

/** A GameSpec rendered for review: the plan card (GAD-020). */
export interface GamePlanView {
  id: string;
  title: string;
  genre: string;
  perspective: string;
  artStyle: string;
  mechanics: string[];
  systems: GamePlanSystem[];
  openQuestions: GamePlanQuestion[];
  approved: boolean;
}

/** One named build in the Versions tab (GAD-022): a journal range with a label. */
export interface GameVersionItem {
  id: string;
  version: string;
  label: string;
  createdAt: string;
  commitHash: string;
  author: string;
  changesCount: number;
}
