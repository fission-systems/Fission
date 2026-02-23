import type { BinaryInfo } from "../types";

interface StatusBarProps {
    binaryInfo: BinaryInfo | null;
    functionCount: number;
    /** Current git branch name, e.g. "main" */
    gitBranch?: string;
    /** Background task progress: value 0–1 and a message */
    progress?: { value: number; message: string } | null;
    /** Whether dynamic (debug) mode is active */
    dynamicMode?: boolean;
    /** Toggle dynamic / static mode */
    onToggleDynamicMode?: () => void;
}

export default function StatusBar({
    binaryInfo,
    functionCount,
    gitBranch,
    progress,
    dynamicMode = false,
    onToggleDynamicMode,
}: StatusBarProps) {
    return (
        <div className={`status-bar${dynamicMode ? " status-bar--dynamic" : ""}`}>
            {/* Left group — binary info, grows to fill space */}
            <div className="status-bar__left">
                {binaryInfo ? (
                    <>
                        <span className="status-bar__item">📄 {binaryInfo.name}</span>
                        <span className="status-bar__item">{binaryInfo.format} · {binaryInfo.arch}</span>
                        <span className="status-bar__item">Entry: {binaryInfo.entry_point}</span>
                        <span className="status-bar__item">
                            {functionCount} {functionCount === 1 ? "fn" : "fns"}
                        </span>
                        <span className="status-bar__item">§ {binaryInfo.section_count}</span>
                    </>
                ) : (
                    <span className="status-bar__item status-bar__item--muted">No binary loaded</span>
                )}
            </div>

            {/* Center — progress (only when active) */}
            {progress && (
                <div className="status-bar__item status-bar__progress-area">
                    <span className="status-bar__progress-msg">{progress.message}</span>
                    <div className="status-bar__progress-bar">
                        <div
                            className="status-bar__progress-fill"
                            style={{ width: `${Math.round(progress.value * 100)}%` }}
                        />
                    </div>
                    <span className="status-bar__progress-pct">
                        {Math.round(progress.value * 100)}%
                    </span>
                </div>
            )}

            {/* Right group — mode toggle + git (row-reverse so leftmost in DOM = rightmost visually) */}
            <div className="status-bar__right">
                {gitBranch && gitBranch !== "—" && (
                    <span className="status-bar__item status-bar__git">⎇ {gitBranch}</span>
                )}
                <button
                    className={`status-bar__item status-bar__mode-btn${dynamicMode ? " status-bar__mode-btn--dynamic" : ""}`}
                    onClick={onToggleDynamicMode}
                    title={dynamicMode ? "Switch to Static Analysis mode" : "Switch to Dynamic (Debug) mode"}
                >
                    {dynamicMode ? "⚡ Dynamic" : "🔍 Static"}
                </button>
            </div>
        </div>
    );
}
