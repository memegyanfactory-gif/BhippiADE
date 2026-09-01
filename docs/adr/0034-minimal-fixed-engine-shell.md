# ADR-0034 — Minimal fixed engine shell before advanced docking

- **Status:** Accepted
- **Date:** 2026-09-01
- **Relates to:** ADR-0020, ADR-0028, INV-034, INV-073; ENG-140, ENG-240…254

## Context

The Engine pane has the right core surfaces—Outliner, viewport, Inspector, Content Drawer,
Output Log, command palette and HUD editor—but its top toolbar exposes almost every option at
the same visual level. Transport options, transform tools, four snap values, undo/redo, six
shading modes, Show flags, camera speed, seven camera views, FOV, screen percentage, maximise,
AI mode, capabilities, weather, Add, Scene/HUD, Content, Log and reload compete in one strip.

ENG-140 originally proposed a dockable/tabbed/floating system. That would make the crowded
surfaces movable without first establishing a calm default workflow. It would also multiply
layout persistence, focus, minimum-size, narrow-window and recovery states.

## Decision

Bhippi will first ship a fixed, preset-driven editor shell:

1. A single quiet application toolbar contains project/scene/save, centred Play/Pause/Stop,
   and Add/AI/More on the right.
2. Scene/HUD and future authoring modes use one narrow mode rail. A mode replaces contextual
   content; it does not add a permanent panel.
3. The viewport has one compact contextual strip for transform, snap, view and display options.
4. The Outliner is the single left scene hierarchy and the Inspector is the single persistent
   right property surface.
5. Content, Output, Problems, AI Activity and Game Debug converge into one bottom drawer.
6. Advanced and infrequent controls remain reachable through context menus, popovers, the
   shared command registry and keyboard shortcuts.
7. At 1440 px the primary toolbar may expose no more than nine actionable controls and may not
   wrap. The viewport remains the largest region.

This supersedes ENG-140's docking-first acceptance. Advanced tab dragging/docking may be
reconsidered only after the fixed shell passes the task, accessibility, narrow-window and
corrupt-layout recovery fixtures. Floating windows are not required for an Unreal-familiar
workflow.

## Visual system

Editor chrome uses neutral charcoal surfaces, one amber product accent and semantic status
colours only. Controls follow a 4/8 px spacing rhythm and 28/32 px compact heights. Hierarchy,
spacing and labels carry emphasis; decorative glow, gradients and large shadows do not compete
with authored game content. Appearance themes may vary tokens but not information hierarchy.

## Behaviour preservation

This is an information-architecture change. It does not move engine business logic into
TypeScript, change document ownership, create a second command catalogue, or remove a keyboard,
palette, accessibility or AI path. Toolbar and menu controls invoke the same existing handlers.

## Evidence required

- before/after screenshots at 1366×768, 1440×900 and 1920×1080;
- visible-control and viewport-area comparison;
- five timed tasks: transform, add, assign material, play/stop, inspect game-debug report;
- no toolbar wrapping or main-shell horizontal scrolling;
- loading/empty/error/populated/Play/narrow states;
- axe zero serious/critical, keyboard-only completion and reduced-motion check.

## Consequences

- New subsystem work may not add a permanent toolbar control or standalone panel by default.
- The command palette becomes the complete expert fallback and must share command handlers.
- Some controls require one extra click, but common actions remain one-click and shortcuts stay.
- ENG-140 remains unimplemented as a docking system and is superseded by ENG-241 for the default
  shell; this does not falsely mark docking complete.

## Alternatives rejected

- **Keep adding toolbar groups.** Discoverability collapses when every action is prominent.
- **Build docking first.** It multiplies states while preserving the underlying clutter.
- **Hide features without alternate routes.** Minimalism cannot mean lost capability.
- **Copy Unreal/Unity pixel-for-pixel.** Their hierarchy is useful; their branding, density and
  legacy complexity are not requirements for Bhippi.
