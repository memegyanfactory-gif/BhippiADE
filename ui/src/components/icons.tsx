/**
 * The Bhippi icon family.
 *
 * One unified vector icon system drawn to precision rules:
 * - 24×24 box, 20×20 live geometric area with consistent padding
 * - 1.6px default stroke, round caps and joins, sleek curvature
 * - Half-pixel snapping for ultra-crisp display on high and standard DPI displays
 * - Color always inherits from `currentColor`
 */

type IconProps = { size?: number; className?: string };

const stroke = (size: number, width = 1.6) => ({
  width: size,
  height: size,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: width,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
});

const solid = (size: number) => ({
  width: size,
  height: size,
  viewBox: "0 0 24 24",
  fill: "currentColor",
  "aria-hidden": true,
});

/* ── Composer and turn actions ─────────────────────────────────────────── */

export const IconSend = ({ size = 15 }: IconProps) => (
  <svg {...stroke(size, 1.8)}>
    <path d="M12 19V5" />
    <path d="m6 11 6-6 6 6" />
  </svg>
);

export const IconStop = ({ size = 12 }: IconProps) => (
  <svg {...solid(size)}>
    <rect x="6" y="6" width="12" height="12" rx="2.5" />
  </svg>
);

export const IconQueue = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <path d="M4 6h16M4 12h10M4 18h6" />
    <path d="m16 15 3 3-3 3" />
  </svg>
);

export const IconCopy = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size)}>
    <rect x="9" y="9" width="11" height="11" rx="2.5" />
    <path d="M15 5.5A1.5 1.5 0 0 0 13.5 4h-8A1.5 1.5 0 0 0 4 5.5v8A1.5 1.5 0 0 0 5.5 15" />
  </svg>
);

export const IconCheck = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 2)}>
    <path d="m4.5 12.5 4.8 4.8L19.5 7" />
  </svg>
);

export const IconRefresh = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.7)}>
    <path d="M19.4 9.6A8 8 0 0 0 5.9 6.4L4 8.3" />
    <path d="M4 4.6v3.9h3.9" />
    <path d="M4.6 14.4a8 8 0 0 0 13.5 3.2l1.9-1.9" />
    <path d="M20 19.4v-3.9h-3.9" />
  </svg>
);

export const IconEdit = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size)}>
    <path d="M4 20h4.5L20 8.5a2.1 2.1 0 0 0 0-3L18.5 4a2.1 2.1 0 0 0-3 0L4 15.5z" />
    <path d="m14.5 5.5 4 4" />
  </svg>
);

export const IconTrash = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <path d="M4 7h16" />
    <path d="M10 11v6M14 11v6" />
    <path d="M6.5 7l.8 11.5a2 2 0 0 0 2 1.5h5.4a2 2 0 0 0 2-1.5L17.5 7" />
    <path d="M9 7V4.5a1.5 1.5 0 0 1 1.5-1.5h3A1.5 1.5 0 0 1 15 4.5V7" />
  </svg>
);

export const IconPlus = ({ size = 15 }: IconProps) => (
  <svg {...stroke(size, 1.8)}>
    <path d="M12 5v14M5 12h14" />
  </svg>
);

export const IconClose = ({ size = 13 }: IconProps) => (
  <svg {...stroke(size, 1.8)}>
    <path d="M6 6 18 18M18 6 6 18" />
  </svg>
);

/* ── Permissions & Modes ──────────────────────────────────────────────── */

export const IconHand = ({ size = 15 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <path d="M18 11V6a2 2 0 0 0-4 0v4M14 10V4a2 2 0 0 0-4 0v6M10 10.5V6a2 2 0 0 0-4 0v8" />
    <path d="M6 14a6 6 0 0 0 12 0v-3a2 2 0 0 0-4 0" />
    <path d="M6 14v1a6 6 0 0 0 6 6h2a6 6 0 0 0 6-6v-4" />
  </svg>
);

export const IconBolt = ({ size = 15 }: IconProps) => (
  <svg {...stroke(size, 1.7)}>
    <path d="M13 2 4 14h7l-2 8 11-12h-7z" />
  </svg>
);

export const IconShield = ({ size = 15 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <path d="M12 3s7 2.5 7 7c0 5.5-4 9.5-7 11-3-1.5-7-5.5-7-11 0-4.5 7-7 7-7z" />
  </svg>
);

export const IconShieldAlert = ({ size = 15 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <path d="M12 3s7 2.5 7 7c0 5.5-4 9.5-7 11-3-1.5-7-5.5-7-11 0-4.5 7-7 7-7z" />
    <path d="M12 8v4.5" strokeWidth="1.8" />
    <circle cx="12" cy="15.5" r="0.75" fill="currentColor" stroke="none" />
  </svg>
);

export const IconShieldCheck = ({ size = 15 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <path d="M12 3s7 2.5 7 7c0 5.5-4 9.5-7 11-3-1.5-7-5.5-7-11 0-4.5 7-7 7-7z" />
    <path d="m9 12 2 2 4-4" strokeWidth="1.8" />
  </svg>
);

export const IconMonitor = ({ size = 15 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <rect x="3" y="4" width="18" height="12" rx="2" />
    <path d="M8 20h8M12 16v4" />
  </svg>
);

export const IconVision = ({ size = 14, className }: IconProps) => (
  <svg {...stroke(size, 1.6)} className={className}>
    <path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7-10-7-10-7z" />
    <circle cx="12" cy="12" r="3" />
  </svg>
);

export const IconEye = IconVision;

/* ── Chrome ────────────────────────────────────────────────────────────── */

export const IconGear = ({ size = 15 }: IconProps) => (
  <svg {...stroke(size)}>
    <circle cx="12" cy="12" r="3.2" />
    <path d="M12 3.5h.9l.5 2.3a6.6 6.6 0 0 1 1.9 1.1l2.2-.8.9 1.6-1.7 1.6a6.7 6.7 0 0 1 0 2.4l1.7 1.6-.9 1.6-2.2-.8a6.6 6.6 0 0 1-1.9 1.1l-.5 2.3h-1.8l-.5-2.3a6.6 6.6 0 0 1-1.9-1.1l-2.2.8-.9-1.6 1.7-1.6a6.7 6.7 0 0 1 0-2.4L5.6 7.7l.9-1.6 2.2.8a6.6 6.6 0 0 1 1.9-1.1l.5-2.3z" />
  </svg>
);

export const IconChevronDown = ({ size = 13 }: IconProps) => (
  <svg {...stroke(size, 1.8)}>
    <path d="m6 9 6 6 6-6" />
  </svg>
);

export const IconChevronLeft = ({ size = 13 }: IconProps) => (
  <svg {...stroke(size, 1.8)}>
    <path d="m15 18-6-6 6-6" />
  </svg>
);

export const IconChevronRight = ({ size = 13 }: IconProps) => (
  <svg {...stroke(size, 1.8)}>
    <path d="m9 6 6 6-6 6" />
  </svg>
);

export const IconChevronUp = ({ size = 13 }: IconProps) => (
  <svg {...stroke(size, 1.8)}>
    <path d="m18 15-6-6-6 6" />
  </svg>
);

export const IconGripVertical = ({ size = 14 }: IconProps) => (
  <svg {...solid(size)}>
    <circle cx="9" cy="6" r="1.5" />
    <circle cx="15" cy="6" r="1.5" />
    <circle cx="9" cy="12" r="1.5" />
    <circle cx="15" cy="12" r="1.5" />
    <circle cx="9" cy="18" r="1.5" />
    <circle cx="15" cy="18" r="1.5" />
  </svg>
);

export const IconGitMerge = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <circle cx="18" cy="18" r="3" />
    <circle cx="6" cy="6" r="3" />
    <circle cx="6" cy="18" r="3" />
    <path d="M6 9v6" />
    <path d="M9 6h4a5 5 0 0 1 5 5v4" />
  </svg>
);

export const IconSplitView = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.5)}>
    <rect x="3.5" y="4.5" width="17" height="15" rx="2" />
    <path d="M12 4.5v15" />
  </svg>
);

export const IconPanelLeft = ({ size = 15 }: IconProps) => (
  <svg {...stroke(size, 1.5)}>
    <rect x="3.5" y="4.5" width="17" height="15" rx="2.5" />
    <path d="M10 4.5v15" />
    <path d="M6.2 9h1.6M6.2 12h1.6" strokeWidth="1.3" />
  </svg>
);

export const IconPanelRight = ({ size = 15 }: IconProps) => (
  <svg {...stroke(size, 1.5)}>
    <rect x="3.5" y="4.5" width="17" height="15" rx="2.5" />
    <path d="M14 4.5v15" />
    <path d="M16.2 9h1.6M16.2 12h1.6" strokeWidth="1.3" />
  </svg>
);

export const IconMinimize = ({ size = 12 }: IconProps) => (
  <svg {...stroke(size, 1.5)}>
    <path d="M5.5 12h13" />
  </svg>
);

export const IconMaximize = ({ size = 11 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <rect x="5.5" y="5.5" width="13" height="13" rx="2" />
  </svg>
);

export const IconArrowLeft = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <path d="M19 12H5.5" />
    <path d="m11 5.5-5.5 6.5L11 18.5" />
  </svg>
);

export const IconArrowRight = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <path d="M5 12h13.5" />
    <path d="m13 5.5 5.5 6.5L13 18.5" />
  </svg>
);

export const IconArrowUp = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 2)}>
    <line x1="12" y1="19" x2="12" y2="5" />
    <polyline points="5 12 12 5 19 12" />
  </svg>
);

export const IconBullseye = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <circle cx="12" cy="12" r="8" />
    <circle cx="12" cy="12" r="2.5" fill="currentColor" stroke="none" />
  </svg>
);

export const IconSliders = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <line x1="4" y1="21" x2="4" y2="14" />
    <line x1="4" y1="10" x2="4" y2="3" />
    <line x1="12" y1="21" x2="12" y2="12" />
    <line x1="12" y1="8" x2="12" y2="3" />
    <line x1="20" y1="21" x2="20" y2="16" />
    <line x1="20" y1="12" x2="20" y2="3" />
    <line x1="1" y1="14" x2="7" y2="14" />
    <line x1="9" y1="8" x2="15" y2="8" />
    <line x1="17" y1="16" x2="23" y2="16" />
  </svg>
);

export const IconSidebar = ({ size = 15 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <rect x="3.5" y="4" width="17" height="16" rx="2.8" />
    <path d="M9 4v16" />
  </svg>
);

export const IconPalette = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <circle cx="13.5" cy="6.5" r=".5" fill="currentColor" />
    <circle cx="17.5" cy="10.5" r=".5" fill="currentColor" />
    <circle cx="8.5" cy="7.5" r=".5" fill="currentColor" />
    <circle cx="6.5" cy="12.5" r=".5" fill="currentColor" />
    <path d="M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c.9 0 1.5-.7 1.5-1.5 0-.4-.2-.8-.4-1.1-.3-.3-.4-.7-.4-1.1 0-.8.7-1.5 1.5-1.5H16c3.3 0 6-2.7 6-6 0-5.5-4.5-9.8-10-8.8z" />
  </svg>
);

export const IconExternalLink = ({ size = 13 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
    <polyline points="15 3 21 3 21 9" />
    <line x1="10" y1="14" x2="21" y2="3" />
  </svg>
);

export const IconSearch = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <circle cx="11" cy="11" r="6.5" />
    <path d="m16 16 4.5 4.5" />
  </svg>
);

export const IconSearchWeb = IconSearch;

/* ── Navigation ────────────────────────────────────────────────────────── */

export const IconChat = ({ size = 15 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <path d="M20 13.5a3 3 0 0 1-3 3H9.6L5 20v-3.5H7a3 3 0 0 1-3-3v-6a3 3 0 0 1 3-3h10a3 3 0 0 1 3 3z" />
    <path d="M8.5 9.5h7M8.5 12.5h4.5" strokeWidth="1.3" />
  </svg>
);

export const IconTimer = ({ size = 15 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <circle cx="12" cy="13.5" r="7" />
    <path d="M12 10v3.5l2.4 1.4" />
    <path d="M9.8 3.5h4.4" />
    <path d="M12 3.5v3" strokeWidth="1.3" />
  </svg>
);

export const IconLibrary = ({ size = 15 }: IconProps) => (
  <svg {...stroke(size, 1.5)}>
    <rect x="4" y="5" width="4" height="14" rx="1.2" />
    <rect x="9.5" y="5" width="4" height="14" rx="1.2" />
    <path d="m15.6 6.4 2.9.8a1.2 1.2 0 0 1 .8 1.5l-2.9 10.4-4.1-1.1" />
  </svg>
);

/* ── Engine activity ───────────────────────────────────────────────────── */

export const IconPlan = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <path d="M9 6.5h11M9 12h11M9 17.5h7" />
    <path d="M4.5 6.5h.01M4.5 12h.01M4.5 17.5h.01" strokeWidth="2.4" />
  </svg>
);

export const IconReadSource = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.5)}>
    <path d="M12 7.2C10.6 5.9 8.7 5.3 6 5.4a1 1 0 0 0-1 1v10.2a1 1 0 0 0 1 1c2.7-.1 4.6.5 6 1.8" />
    <path d="M12 7.2c1.4-1.3 3.3-1.9 6-1.8a1 1 0 0 1 1 1v10.2a1 1 0 0 1-1 1c-2.7-.1-4.6.5-6 1.8" />
    <path d="M12 7.2v12.2" strokeWidth="1.3" />
  </svg>
);

export const IconFetchUrl = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <circle cx="12" cy="12" r="8" />
    <path d="M4.2 12h15.6" />
    <path d="M12 4c2 2.3 3.1 5 3.1 8s-1.1 5.7-3.1 8c-2-2.3-3.1-5-3.1-8s1.1-5.7 3.1-8Z" />
  </svg>
);

export const IconExtractDots = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.5)}>
    <path d="m8.9 7.6 6.4 1.2M7.7 9.9l1.5 6M16.3 11.2l-5.2 5.4" strokeWidth="1.3" />
    <circle cx="7" cy="6.2" r="2.2" />
    <circle cx="17.2" cy="9.4" r="2.2" />
    <circle cx="9.6" cy="17.8" r="2.2" />
  </svg>
);

export const IconCheckProviders = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.5)}>
    <rect x="3.5" y="4.5" width="17" height="6.5" rx="2" />
    <rect x="3.5" y="13" width="17" height="6.5" rx="2" />
    <path d="M7 7.8h.01M7 16.2h.01" strokeWidth="2.2" />
  </svg>
);

/* ── Project, source control, tooling ──────────────────────────────────── */

export const IconFolder = ({ size = 16 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <path d="M3 7a2 2 0 0 1 2-2h3.5l1.8 2H19a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
  </svg>
);

export const IconFolderOpen = ({ size = 16 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <path d="M3.5 18.5V6.5a2 2 0 0 1 2-2h3.1a2 2 0 0 1 1.5.7l1.2 1.4h6.2a2 2 0 0 1 2 2v1.9" />
    <path d="M3.5 18.5 6.2 11a1.6 1.6 0 0 1 1.5-1h12.1a1 1 0 0 1 .9 1.4l-2.4 6.7a1.6 1.6 0 0 1-1.5 1.1H5.5a2 2 0 0 1-2-1.7Z" />
  </svg>
);

export const IconGitBranch = ({ size = 15 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <circle cx="7" cy="5.8" r="2.3" />
    <circle cx="7" cy="18.2" r="2.3" />
    <circle cx="17" cy="8.6" r="2.3" />
    <path d="M7 8.1v7.8" />
    <path d="M17 10.9c0 2.6-2.8 4-6.2 4.2" />
  </svg>
);

export const IconCode = ({ size = 15 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <path d="m8.2 8-4 4 4 4" />
    <path d="m15.8 8 4 4-4 4" />
    <path d="m13.6 5.5-3.2 13" strokeWidth="1.4" />
  </svg>
);

export const IconTerminal = ({ size = 15 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <rect x="3.5" y="4.5" width="17" height="15" rx="2.5" />
    <path d="m7.5 9.8 2.6 2.4-2.6 2.4" />
    <path d="M13 15h3.5" />
  </svg>
);

export const IconExternal = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <path d="M14 4.5h5.5V10" />
    <path d="M19.5 4.5 12 12" />
    <path d="M18 14v4.5a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2v-10a2 2 0 0 1 2-2h4.5" />
  </svg>
);

/* ── Workbench ─────────────────────────────────────────────────────────── */

export const IconEditor = ({ size = 15 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <path d="M13 3.8H6.5a2 2 0 0 0-2 2v12.4a2 2 0 0 0 2 2h11a2 2 0 0 0 2-2V10.3z" />
    <path d="M13 3.8v4.5a2 2 0 0 0 2 2h4.5" />
    <path d="m10 13.4-1.7 1.8L10 17" strokeWidth="1.35" />
    <path d="m13.6 13.4 1.7 1.8-1.7 1.8" strokeWidth="1.35" />
  </svg>
);

export const IconBrowser = ({ size = 15 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <rect x="3.5" y="4.5" width="17" height="15" rx="2.5" />
    <path d="M3.5 9h17" />
    <path d="M6.4 6.7h.01M8.9 6.7h.01M11.4 6.7h.01" strokeWidth="1.7" />
    <path d="M13.6 6.7h4.2" strokeWidth="1.2" />
  </svg>
);

export const IconEngine = ({ size = 15 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <path d="M12 3 4.2 7.6v8.8L12 21l7.8-4.6V7.6z" />
    <path d="M4.2 7.6 12 12.2l7.8-4.6" />
    <path d="M12 12.2V21" />
    <path d="m7.5 5.3 4.5 2.6 4.5-2.6" strokeWidth="1.2" />
  </svg>
);

export const IconReload = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.7)}>
    <path d="M19.5 12a7.5 7.5 0 1 1-2.4-5.5" />
    <path d="M19.6 4.8v4.4h-4.4" />
  </svg>
);

export const IconSave = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <path d="M5.5 4.5h10.2L19.5 8.3v11.2a1 1 0 0 1-1 1h-13a1 1 0 0 1-1-1v-14a1 1 0 0 1 1-1Z" />
    <path d="M8 4.5v4.2h6.5V4.5" />
    <path d="M8 20.5v-5.3h8v5.3" />
  </svg>
);

export const IconRules = ({ size = 15 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <path d="M6.5 3.8h11a1.5 1.5 0 0 1 1.5 1.5v13.4a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 5 18.7V5.3a1.5 1.5 0 0 1 1.5-1.5Z" />
    <path d="M8.4 8.2h7.2M8.4 11.4h5" strokeWidth="1.3" />
    <path d="m8.4 15.6 1.5 1.5 3.2-3.2" strokeWidth="1.4" />
  </svg>
);

export const IconSplit = ({ size = 15 }: IconProps) => (
  <svg {...stroke(size, 1.5)}>
    <rect x="3.5" y="4.5" width="17" height="15" rx="2.5" />
    <path d="M12 4.5v15" strokeDasharray="2.4 2.2" />
  </svg>
);

export const IconDot = ({ size = 8 }: IconProps) => (
  <svg {...solid(size)}>
    <circle cx="12" cy="12" r="6" />
  </svg>
);

/* ── File-type glyphs ─────────────────────────────────────────────────── */

const FileBase = ({ size, children }: { size: number; children?: React.ReactNode }) => (
  <svg {...stroke(size, 1.4)}>
    <path d="M13.2 3.8H7a1.8 1.8 0 0 0-1.8 1.8v12.8A1.8 1.8 0 0 0 7 20.2h10a1.8 1.8 0 0 0 1.8-1.8V9.4z" />
    <path d="M13.2 3.8v3.8a1.8 1.8 0 0 0 1.8 1.8h3.8" />
    {children}
  </svg>
);

export const IconFile = ({ size = 14 }: IconProps) => <FileBase size={size} />;

export const IconFileCode = ({ size = 14 }: IconProps) => (
  <FileBase size={size}>
    <path d="m10 12.6-1.6 1.7 1.6 1.7" strokeWidth="1.25" />
    <path d="m13.8 12.6 1.6 1.7-1.6 1.7" strokeWidth="1.25" />
  </FileBase>
);

export const IconFileText = ({ size = 14 }: IconProps) => (
  <FileBase size={size}>
    <path d="M8.4 12.4h7M8.4 15.2h4.6" strokeWidth="1.25" />
  </FileBase>
);

export const IconFileData = ({ size = 14 }: IconProps) => (
  <FileBase size={size}>
    <path d="M10.4 12.2c-1 0-1 1-1 1.6s0 1-.9 1c.9 0 .9.5.9 1.1s0 1.6 1 1.6" strokeWidth="1.2" />
    <path d="M13.6 12.2c1 0 1 1 1 1.6s0 1 .9 1c-.9 0-.9.5-.9 1.1s0 1.6-1 1.6" strokeWidth="1.2" />
  </FileBase>
);

export const IconFileStyle = ({ size = 14 }: IconProps) => (
  <FileBase size={size}>
    <circle cx="12" cy="14.6" r="2.4" strokeWidth="1.25" />
    <path d="M12 12.2v4.8" strokeWidth="1.25" />
  </FileBase>
);

export const IconFileImage = ({ size = 14 }: IconProps) => (
  <FileBase size={size}>
    <circle cx="10" cy="13.4" r="1" strokeWidth="1.2" />
    <path d="m8.4 17 2.4-2.4 1.6 1.6 1.4-1.2 1.8 2" strokeWidth="1.2" />
  </FileBase>
);

export const IconFileConfig = ({ size = 14 }: IconProps) => (
  <FileBase size={size}>
    <circle cx="12" cy="14.8" r="1.5" strokeWidth="1.2" />
    <path d="M12 11.6v.9M12 17.1v.9M9.2 13.2l.8.5M14 15.9l.8.5M14.8 13.2l-.8.5M10 15.9l-.8.5" strokeWidth="1.1" />
  </FileBase>
);

const FILE_KINDS: Record<string, { icon: (props: IconProps) => JSX.Element; tint: string }> = {
  ts: { icon: IconFileCode, tint: "#5b93d8" },
  tsx: { icon: IconFileCode, tint: "#5b93d8" },
  js: { icon: IconFileCode, tint: "#c9a227" },
  jsx: { icon: IconFileCode, tint: "#c9a227" },
  mjs: { icon: IconFileCode, tint: "#c9a227" },
  cjs: { icon: IconFileCode, tint: "#c9a227" },
  rs: { icon: IconFileCode, tint: "#cf8250" },
  py: { icon: IconFileCode, tint: "#57a05f" },
  go: { icon: IconFileCode, tint: "#4fa3b8" },
  java: { icon: IconFileCode, tint: "#c07a5b" },
  rb: { icon: IconFileCode, tint: "#c96a6a" },
  php: { icon: IconFileCode, tint: "#8079c4" },
  c: { icon: IconFileCode, tint: "#6f8fb8" },
  h: { icon: IconFileCode, tint: "#6f8fb8" },
  cpp: { icon: IconFileCode, tint: "#6f8fb8" },
  sh: { icon: IconTerminal, tint: "#8a9a7b" },
  ps1: { icon: IconTerminal, tint: "#6f8fb8" },
  html: { icon: IconFileCode, tint: "#cf7a4e" },
  css: { icon: IconFileStyle, tint: "#5f8fc4" },
  scss: { icon: IconFileStyle, tint: "#c07a9a" },
  json: { icon: IconFileData, tint: "#c9a227" },
  toml: { icon: IconFileConfig, tint: "#9a8f7b" },
  yaml: { icon: IconFileConfig, tint: "#9a8f7b" },
  yml: { icon: IconFileConfig, tint: "#9a8f7b" },
  lock: { icon: IconFileConfig, tint: "#7d766c" },
  md: { icon: IconFileText, tint: "#7f9ec4" },
  mdx: { icon: IconFileText, tint: "#7f9ec4" },
  txt: { icon: IconFileText, tint: "#8f887e" },
  sql: { icon: IconFileData, tint: "#7fa5b8" },
  svg: { icon: IconFileImage, tint: "#b08bc0" },
  png: { icon: IconFileImage, tint: "#7fa88f" },
  jpg: { icon: IconFileImage, tint: "#7fa88f" },
  jpeg: { icon: IconFileImage, tint: "#7fa88f" },
  gif: { icon: IconFileImage, tint: "#7fa88f" },
  webp: { icon: IconFileImage, tint: "#7fa88f" },
  ico: { icon: IconFileImage, tint: "#7fa88f" },
};

const FILE_NAMES: Record<string, { icon: (props: IconProps) => JSX.Element; tint: string }> = {
  "package.json": { icon: IconFileData, tint: "#8fae6a" },
  "cargo.toml": { icon: IconFileConfig, tint: "#cf8250" },
  "cargo.lock": { icon: IconFileConfig, tint: "#7d766c" },
  dockerfile: { icon: IconFileConfig, tint: "#5f8fc4" },
  ".gitignore": { icon: IconGitBranch, tint: "#a08878" },
  "readme.md": { icon: IconFileText, tint: "#9fb8d4" },
  "claude.md": { icon: IconRules, tint: "#c9975a" },
  "agents.md": { icon: IconRules, tint: "#c9975a" },
};

export function FileGlyph({ name, size = 14 }: { name: string; size?: number }) {
  const lower = name.toLowerCase();
  const byName = FILE_NAMES[lower];
  const extension = lower.includes(".") ? (lower.split(".").pop() ?? "") : "";
  const kind = byName ?? FILE_KINDS[extension];
  if (!kind) return <IconFile size={size} />;
  const Glyph = kind.icon;
  return (
    <span style={{ color: kind.tint, display: "grid" }}>
      <Glyph size={size} />
    </span>
  );
}

export const IconSparkle = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <path d="m12 3 2.8 6.2L21 12l-6.2 2.8L12 21l-2.8-6.2L3 12l6.2-2.8z" />
  </svg>
);

export const IconBrain = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.5)}>
    <path d="M9.5 4a3.5 3.5 0 0 0-3.5 3.5c0 .7.2 1.4.6 2A3.5 3.5 0 0 0 5 13a3.5 3.5 0 0 0 3 3.5V19a1 1 0 0 0 1 1h.5a3.5 3.5 0 0 0 3.5-3.5V13" />
    <path d="M14.5 4a3.5 3.5 0 0 1 3.5 3.5c0 .7-.2 1.4-.6 2a3.5 3.5 0 0 1 1.6 3.5 3.5 3.5 0 0 1-3 3.5V19a1 1 0 0 1-1 1h-.5a3.5 3.5 0 0 1-3.5-3.5V13" />
  </svg>
);

/* ── Fault cards and remedies ─────────────────────────────────────────── */

export const IconAlert = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.7)}>
    <path d="M12 4.5 3.2 19a1 1 0 0 0 .87 1.5h15.86A1 1 0 0 0 20.8 19z" />
    <path d="M12 10v4" strokeWidth="1.8" />
    <circle cx="12" cy="17" r="0.8" fill="currentColor" stroke="none" />
  </svg>
);

export const IconClock = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <circle cx="12" cy="12" r="8.5" />
    <path d="M12 7.5V12l3 1.8" />
  </svg>
);

export const IconDownload = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <path d="M12 3.5v10" />
    <path d="m8 10 4 4 4-4" />
    <path d="M4.5 17v2a1.5 1.5 0 0 0 1.5 1.5h12a1.5 1.5 0 0 0 1.5-1.5v-2" />
  </svg>
);

export const IconKey = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <circle cx="8" cy="8" r="4" />
    <path d="m10.9 10.9 8.6 8.6" />
    <path d="m17 17 1.8-1.8" />
  </svg>
);

export const IconShrink = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <path d="M4 6.5v11" />
    <path d="M20 6.5v11" />
    <path d="M8.5 12h7" />
    <path d="m11 9.5-2.5 2.5L11 14.5" />
    <path d="m13 9.5 2.5 2.5L13 14.5" />
  </svg>
);

export const IconSwap = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <path d="M4 9h13" />
    <path d="m14 6 3 3-3 3" />
    <path d="M20 15H7" />
    <path d="m10 12-3 3 3 3" />
  </svg>
);

/* ── Composer bar ─────────────────────────────────────────────────────── */

export const IconAttach = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <path d="M20 11.5 12.3 19.2a4.5 4.5 0 0 1-6.4-6.4l7.8-7.8a3 3 0 0 1 4.2 4.2l-7.7 7.8a1.5 1.5 0 0 1-2.2-2.1l7.1-7.2" />
  </svg>
);

export const IconAt = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <circle cx="12" cy="12" r="3.6" />
    <path d="M15.6 8.4V13a2.6 2.6 0 0 0 5.2 0v-1a8.8 8.8 0 1 0-3.4 7" />
  </svg>
);

export const IconSlash = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.8)}>
    <path d="M15.5 4.5 8.5 19.5" />
  </svg>
);

export const IconGauge = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <path d="M4 16.5a8.5 8.5 0 1 1 16 0" />
    <path d="m12 16.5 4-4.5" strokeWidth="1.8" />
    <circle cx="12" cy="16.5" r="1.4" fill="currentColor" stroke="none" />
  </svg>
);

export const IconLayers = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <path d="m12 3.5 8 4.2-8 4.3-8-4.3z" />
    <path d="m4 12 8 4.3 8-4.3" />
    <path d="m4 16.2 8 4.3 8-4.3" />
  </svg>
);

export const IconImage = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.5)}>
    <rect x="3.5" y="4.5" width="17" height="15" rx="2" />
    <circle cx="8.5" cy="9.5" r="1.6" />
    <path d="m4 16.5 4.6-4.2a1.6 1.6 0 0 1 2.2 0L15 16.5" />
    <path d="m13.5 14 1.9-1.7a1.6 1.6 0 0 1 2.2 0L20 14.5" />
  </svg>
);

export const IconMic = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <rect x="9.2" y="3" width="5.6" height="11" rx="2.8" />
    <path d="M5.5 11.5a6.5 6.5 0 0 0 13 0" />
    <path d="M12 18v3" />
  </svg>
);

export const IconMicOff = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <line x1="2" y1="2" x2="22" y2="22" />
    <path d="M18.89 13.23A7.12 7.12 0 0 0 19 12v-2" />
    <path d="M5 10v2a7 7 0 0 0 12 5" />
    <path d="M15 9.34V5a3 3 0 0 0-5.68-1.33" />
    <path d="M9 9v3a3 3 0 0 0 5.12 2.12" />
    <line x1="12" y1="19" x2="12" y2="22" />
  </svg>
);

export const IconVolume = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5" />
    <path d="M15.54 8.46a5 5 0 0 1 0 7.07" />
    <path d="M19.07 4.93a10 10 0 0 1 0 14.14" />
  </svg>
);

export const IconWaveform = ({ size = 14 }: IconProps) => (
  <svg {...stroke(size, 1.6)}>
    <path d="M3 10v4M6 7v10M9 4v16M12 2v20M15 6v12M18 8v8M21 11v2" />
  </svg>
);


/* ── Profile, Activation & Settings ────────────────────────────────────── */

export const IconCrown = ({ size = 14, className }: IconProps) => (
  <svg {...stroke(size, 1.6)} className={className}>
    <path d="M4 18h16M5 18l-2-10 6 4 3-7 3 7 6-4-2 10H5z" />
    <circle cx="3" cy="8" r="0.8" fill="currentColor" />
    <circle cx="12" cy="5" r="0.8" fill="currentColor" />
    <circle cx="21" cy="8" r="0.8" fill="currentColor" />
  </svg>
);

export const IconUser = ({ size = 14, className }: IconProps) => (
  <svg {...stroke(size, 1.6)} className={className}>
    <path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2" />
    <circle cx="12" cy="7" r="4" />
  </svg>
);

export const IconEyeOff = ({ size = 14, className }: IconProps) => (
  <svg {...stroke(size, 1.6)} className={className}>
    <path d="M17.94 17.94A10.07 10.07 0 0 1 12 19c-7 0-10-7-10-7a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 10 7 10 7a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24" />
    <path d="m1 1 22 22" />
  </svg>
);

export const IconCamera = ({ size = 14, className }: IconProps) => (
  <svg {...stroke(size, 1.6)} className={className}>
    <path d="M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z" />
    <circle cx="12" cy="13" r="3" />
  </svg>
);

export const IconSparkles = ({ size = 14, className }: IconProps) => (
  <svg {...stroke(size, 1.6)} className={className}>
    <path d="m12 3 1.9 4.9L19 10l-5.1 2.1L12 17l-1.9-4.9L5 10l5.1-2.1zM5 19l.8 2.2L8 22l-2.2.8L5 25l-.8-2.2L2 22l2.2-.8zM19 19l.8 2.2L22 22l-2.2.8L19 25l-.8-2.2L16 22l2.2-.8z" />
  </svg>
);

export const IconPower = ({ size = 13, className }: IconProps) => (
  <svg {...stroke(size, 1.7)} className={className}>
    <path d="M12 3.5v8" />
    <path d="M6.9 6.5a8 8 0 1 0 10.2 0" />
  </svg>
);

export const IconBadgeCheck = ({ size = 14, className }: IconProps) => (
  <svg {...stroke(size, 1.6)} className={className}>
    <path d="M12 2l2.4 2.4 3.4-.4 1.3 3.1 3 1.6-.8 3.3 1.7 3-2.1 2.7.2 3.4-3.3 1-1.6 3-3-1.2-3 1.2-1.6-3-3.3-1 .2-3.4-2.1-2.7 1.7-3-.8-3.3 3-1.6 1.3-3.1 3.4.4z" />
    <path d="m9 12 2 2 4-4" strokeWidth="1.8" />
  </svg>
);

export const IconStar = ({ size = 14, className }: IconProps) => (
  <svg {...stroke(size, 1.6)} className={className}>
    <path d="m12 3.5 2.1 4.6 4.9.7-3.6 3.4.8 5-4.2-2.3-4.2 2.3.8-5-3.6-3.4 4.9-.7z" />
  </svg>
);

export const IconStarFilled = ({ size = 14, className }: IconProps) => (
  <svg {...solid(size)} className={className}>
    <path d="m12 3.3 2.4 4.7 5.1.7-3.7 3.5.9 5-4.7-2.5-4.7 2.5.9-5-3.7-3.5 5.1-.7z" />
  </svg>
);

export const IconFree = ({ size = 12, className }: IconProps) => (
  <svg {...stroke(size, 1.7)} className={className}>
    <path d="M7 8.5a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v1a2 2 0 0 1-2 2h-2a2 2 0 0 0-2 2v1" />
    <circle cx="12" cy="17.2" r="0.7" fill="currentColor" stroke="none" />
  </svg>
);

export const IconGift = ({ size = 12, className }: IconProps) => (
  <svg {...stroke(size, 1.5)} className={className}>
    <rect x="3.5" y="8.5" width="17" height="10" rx="1.8" />
    <path d="M12 8.5v10M3.5 12.5h17" />
    <path d="M12 8.5c0-2.2-1.6-4-3.7-4A3.2 3.2 0 0 0 5 7.3c0 1.2 1.1 1.8 2.1 1.2L12 8.5z" />
    <path d="M12 8.5c0-2.2 1.6-4 3.7-4A3.2 3.2 0 0 1 19 7.3c0 1.2-1.1 1.8-2.1 1.2L12 8.5z" />
  </svg>
);

export const IconSettings = IconGear;

export const IconPlay = ({ size = 14, className }: IconProps) => (
  <svg {...stroke(size, 1.8)} className={className}>
    <polygon points="5 3 19 12 5 21 5 3" fill="currentColor" />
  </svg>
);

export const IconPause = ({ size = 14, className }: IconProps) => (
  <svg {...stroke(size, 1.8)} className={className}>
    <rect x="6" y="4" width="4" height="16" fill="currentColor" />
    <rect x="14" y="4" width="4" height="16" fill="currentColor" />
  </svg>
);

export const IconBox = ({ size = 14, className }: IconProps) => (
  <svg {...stroke(size, 1.6)} className={className}>
    <path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z" />
    <polyline points="3.27 6.96 12 12.01 20.73 6.96" />
    <line x1="12" y1="22.08" x2="12" y2="12" />
  </svg>
);

export const IconGrid = ({ size = 14, className }: IconProps) => (
  <svg {...stroke(size, 1.6)} className={className}>
    <rect x="3" y="3" width="7" height="7" rx="1" />
    <rect x="14" y="3" width="7" height="7" rx="1" />
    <rect x="14" y="14" width="7" height="7" rx="1" />
    <rect x="3" y="14" width="7" height="7" rx="1" />
  </svg>
);

export const IconSun = ({ size = 14, className }: IconProps) => (
  <svg {...stroke(size, 1.6)} className={className}>
    <circle cx="12" cy="12" r="4" />
    <path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41" />
  </svg>
);

export const IconMaximize2 = ({ size = 14, className }: IconProps) => (
  <svg {...stroke(size, 1.7)} className={className}>
    <polyline points="15 3 21 3 21 9" />
    <polyline points="9 21 3 21 3 15" />
    <line x1="21" y1="3" x2="14" y2="10" />
    <line x1="3" y1="21" x2="10" y2="14" />
  </svg>
);

export const IconMinimize2 = ({ size = 14, className }: IconProps) => (
  <svg {...stroke(size, 1.7)} className={className}>
    <polyline points="4 14 10 14 10 20" />
    <polyline points="20 10 14 10 14 4" />
    <line x1="14" y1="10" x2="21" y2="3" />
    <line x1="3" y1="21" x2="10" y2="14" />
  </svg>
);

export const IconPin = ({ size = 14, className }: IconProps) => (
  <svg {...stroke(size, 1.6)} className={className}>
    <path d="M12 17v5" />
    <path d="M9 10.5V4.5A1.5 1.5 0 0 1 10.5 3h3A1.5 1.5 0 0 1 15 4.5v6l2 2.5v1H7v-1z" />
  </svg>
);

export const IconMove = ({ size = 14, className }: IconProps) => (
  <svg {...stroke(size, 1.6)} className={className}>
    <polyline points="5 9 2 12 5 15" />
    <polyline points="9 5 12 2 15 5" />
    <polyline points="15 19 12 22 9 19" />
    <polyline points="19 9 22 12 19 15" />
    <line x1="2" y1="12" x2="22" y2="12" />
    <line x1="12" y1="2" x2="12" y2="22" />
  </svg>
);

