import type { DebugStateDto } from "../types";

interface TimelinePanelProps {
    debugState: DebugStateDto | null;
    dynamicMode: boolean;
}

export function TimelinePanel({ debugState, dynamicMode }: TimelinePanelProps) {
    if (!dynamicMode) {
        return (
            <div className="timeline-panel timeline-panel--static">
                <p>Timeline is only available in Dynamic (Debug) mode.</p>
                <p className="timeline-panel__hint">
                    Switch to Dynamic mode from the Debug menu or the Status Bar
                    toggle button to start a debug session.
                </p>
            </div>
        );
    }

    const events = debugState?.events ?? [];

    return (
        <div className="timeline-panel">
            <div className="timeline-panel__header">
                <span>🕐 Debug Timeline</span>
                <span
                    className={`timeline-panel__status timeline-panel__status--${debugState?.status ?? "detached"}`}
                >
                    {debugState?.status ?? "detached"}
                </span>
            </div>

            {events.length === 0 ? (
                <div className="timeline-panel__empty">
                    No events recorded yet. Attach to a process to begin.
                </div>
            ) : (
                <ol className="timeline-panel__events">
                    {events.map((evt, i) => (
                        <li key={i} className="timeline-panel__event">
                            <span className="timeline-panel__event-index">{i + 1}</span>
                            <span className="timeline-panel__event-text">{evt}</span>
                        </li>
                    ))}
                </ol>
            )}
        </div>
    );
}
