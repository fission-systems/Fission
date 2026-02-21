import React, { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { DebugStateDto, DebugStatusDto } from "../types";

interface Props {
    onOpenDebugTab: () => void;
    onLog: (msg: string) => void;
}

const STATUS_COLORS: Record<DebugStatusDto, string> = {
    detached: "var(--text-muted)",
    attaching: "#569cd6",
    running: "#4ec94e",
    suspended: "#ddb06c",
    terminated: "#f44747",
};

const STATUS_ICONS: Record<DebugStatusDto, string> = {
    detached: "○",
    attaching: "🔗",
    running: "▶",
    suspended: "⏸",
    terminated: "⏹",
};

export const DebugSidebar: React.FC<Props> = ({ onOpenDebugTab, onLog }) => {
    const [ds, setDs] = useState<DebugStateDto>({
        status: "detached",
        attached_pid: null,
        breakpoints: [],
        registers: null,
        last_event: null,
        events: [],
    });
    const [pidInput, setPidInput] = useState("");
    const [busy, setBusy] = useState(false);

    const fetchState = useCallback(async () => {
        try {
            const state = await invoke<DebugStateDto>("debug_get_state");
            setDs(state);
        } catch { /* non-critical */ }
    }, []);

    useEffect(() => {
        fetchState();
    }, []); // eslint-disable-line react-hooks/exhaustive-deps

    const doAttach = async () => {
        const pid = parseInt(pidInput.trim(), 10);
        if (isNaN(pid) || pid <= 0) return;
        setBusy(true);
        try {
            await invoke("debug_attach", { pid });
            onLog(`[Debug] Attached to PID ${pid}`);
            await fetchState();
            onOpenDebugTab();
        } catch (e) {
            onLog(`[Debug] Attach failed: ${e}`);
        } finally {
            setBusy(false);
        }
    };

    const doDetach = async () => {
        setBusy(true);
        try {
            await invoke("debug_detach");
            onLog("[Debug] Detached");
            await fetchState();
        } catch (e) {
            onLog(`[Debug] Detach failed: ${e}`);
        } finally {
            setBusy(false);
        }
    };

    const isAttached = ds.attached_pid !== null;
    const statusColor = STATUS_COLORS[ds.status];

    return (
        <div className="debug-sidebar">
            <div className="debug-sidebar__header">Debugger</div>

            {/* Status */}
            <div className="debug-sidebar__status" style={{ color: statusColor }}>
                <span className="debug-sidebar__status-icon">{STATUS_ICONS[ds.status]}</span>
                {ds.status.charAt(0).toUpperCase() + ds.status.slice(1)}
                {ds.attached_pid != null && (
                    <span className="debug-sidebar__pid">&nbsp;· PID {ds.attached_pid}</span>
                )}
            </div>

            {/* Attach / Detach */}
            {!isAttached ? (
                <div className="debug-sidebar__attach">
                    <input
                        className="debug-sidebar__input"
                        placeholder="Process PID"
                        value={pidInput}
                        onChange={(e) => setPidInput(e.target.value)}
                        onKeyDown={(e) => e.key === "Enter" && doAttach()}
                    />
                    <button
                        className="debug-sidebar__btn debug-sidebar__btn--attach"
                        onClick={doAttach}
                        disabled={busy || !pidInput.trim()}
                    >
                        🔗 Attach
                    </button>
                </div>
            ) : (
                <div className="debug-sidebar__attach">
                    <button
                        className="debug-sidebar__btn debug-sidebar__btn--detach"
                        onClick={doDetach}
                        disabled={busy}
                    >
                        ⏏ Detach
                    </button>
                </div>
            )}

            {/* Breakpoints summary */}
            <div className="debug-sidebar__section-title">
                Breakpoints ({ds.breakpoints.length})
            </div>
            {ds.breakpoints.length === 0 ? (
                <div className="debug-sidebar__empty">No breakpoints set</div>
            ) : (
                <ul className="debug-sidebar__bp-list">
                    {ds.breakpoints.map((bp) => (
                        <li key={bp.address} className="debug-sidebar__bp-item">
                            <span className={`debug-sidebar__bp-dot ${bp.enabled ? "debug-sidebar__bp-dot--on" : ""}`}>●</span>
                            {bp.address}
                        </li>
                    ))}
                </ul>
            )}

            {/* Last event */}
            {ds.last_event && (
                <div className="debug-sidebar__last-event">
                    📌 {ds.last_event}
                </div>
            )}

            {/* Open debug tab */}
            <button
                className="debug-sidebar__btn debug-sidebar__btn--open"
                onClick={onOpenDebugTab}
            >
                Open Debug Panel ↓
            </button>
        </div>
    );
};

export default DebugSidebar;
