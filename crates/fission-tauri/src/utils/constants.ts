// ── Assembly / Decompile ─────────────────────────────────────────────────────
/** Number of instructions to load per page in the assembly view */
export const ASM_PAGE = 5_000;
/** Decompile invoke timeout in milliseconds */
export const DECOMPILE_TIMEOUT_MS = 30_000;

// ── Hex View ─────────────────────────────────────────────────────────────────
/** Number of bytes to fetch for the hex preview when a function is selected */
export const HEX_PREVIEW_SIZE = 256;

// ── CFG Panel ────────────────────────────────────────────────────────────────
/** Minimum zoom-out scale for the CFG canvas */
export const CFG_MIN_SCALE = 0.15;
/** Maximum zoom-in scale for the CFG canvas */
export const CFG_MAX_SCALE = 5.0;
/** Multiplicative step applied on each scroll tick */
export const CFG_ZOOM_STEP = 1.1;

// ── Listing View ─────────────────────────────────────────────────────────────
/** Distance from the bottom (px) that triggers loading the next page */
export const LOAD_MORE_THRESHOLD_PX = 200;

// ── Console ──────────────────────────────────────────────────────────────────
/** Max number of functions shown by the `funcs` console command */
export const MAX_CONSOLE_FUNCS = 50;
