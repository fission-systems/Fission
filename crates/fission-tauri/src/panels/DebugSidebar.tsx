import React, { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { DebugStateDto, DebugStatusDto, TtdStateDto } from "../types";

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

    // --- Phase 4: Memory dump state ---
    const [memAddr, setMemAddr] = useState("0x");
    const [memSize, setMemSize] = useState("256");
    const [memDump, setMemDump] = useState<string | null>(null);
    const [memBusy, setMemBusy] = useState(false);

    // --- Phase 5: TTD state ---
    const [ttd, setTtd] = useState<TtdStateDto | null>(null);
    const [ttdBusy, setTtdBusy] = useState(false);
    const [ttdSeekInput, setTtdSeekInput] = useState("");

    const fetchState = useCallback(async () => {
        try {
            const state = await invoke<DebugStateDto>("debug_get_state");
            setDs(state);
        } catch { /* non-critical */ }
    }, []);

    const fetchTtd = useCallback(async () => {
        try {
            const s = await invoke<TtdStateDto>("ttd_status");
            setTtd(s);
        } catch { /* non-critical */ }
    }, []);

    useEffect(() => {
        fetchState();
        fetchTtd();
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

    // Phase 4: memory dump
    const doReadMemory = async () => {
        setMemBusy(true);
        setMemDump(null);
        try {
            const dump = await invoke<string>("debug_read_memory", {
                address: memAddr,
                size: parseInt(memSize, 10) || 256,
            });
            setMemDump(dump);
        } catch (e) {
            setMemDump(`Error: ${e}`);
        } finally {
            setMemBusy(false);
        }
    };

    // Phase 5: TTD actions
    const doTtdStart = async () => {
        setTtdBusy(true);
        try {
            const s = await invoke<TtdStateDto>("ttd_start");
            setTtd(s);
            onLog("[TTD] Recording started");
        } catch (e) { onLog(`[TTD] Start failed: ${e}`); }
        finally { setTtdBusy(false); }
    };
    const doTtdStop = async () => {
        setTtdBusy(true);
        try {
            const s = await invoke<TtdStateDto>("ttd_stop");
            setTtd(s);
            onLog(`[TTD] Recording stopped — ${s.snapshot_count} snapshots`);
        } catch (e) { onLog(`[TTD] Stop failed: ${e}`); }
        finally { setTtdBusy(false); }
    };
    const doTtdSeek = async () => {
        const step = parseInt(ttdSeekInput, 10);
        if (isNaN(step)) return;
        setTtdBusy(true);
        try {
            const s = await invoke<TtdStateDto>("ttd_seek", { step });
            setTtd(s);
        } catch (e) { onLog(`[TTD] Seek failed: ${e}`); }
        finally { setTtdBusy(false); }
    };
    const doTtdStep = async (direction: "forward" | "rewind") => {
        setTtdBusy(true);
        try {
            const s = await invoke<TtdStateDto>("ttd_step", { direction });
            setTtd(s);
        } catch (e) { onLog(`[TTD] Step failed: ${e}`); }
        finally { setTtdBusy(false); }
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

            {/* ── Phase 4: Memory Dump ── */}
            <div className="debug-sidebar__section-title">Memory Dump</div>
            <div className="debug-sidebar__mem-row">
                <input
                    className="debug-sidebar__input debug-sidebar__input--addr"
                    placeholder="0x401000"
                    value={memAddr}
                    onChange={(e) => setMemAddr(e.target.value)}
                />
                <select
                    className="debug-sidebar__select"
                    value={memSize}
                    onChange={(e) => setMemSize(e.target.value)}
                >
                    {[64, 128, 256, 512, 1024].map((n) => (
                        <option key={n} value={String(n)}>{n} B</option>
                    ))}
                </select>
            </div>
            <button
                className="debug-sidebar__btn debug-sidebar__btn--read"
                onClick={doReadMemory}
                disabled={memBusy || !isAttached}
                title={isAttached ? "Read process memory" : "Attach to a process first"}
            >
                {memBusy ? "Reading…" : "📋 Read Memory"}
            </button>
            {memDump !== null && (
                <pre className="debug-sidebar__hex-dump">{memDump}</pre>
            )}

            {/* ── Phase 5: TTD Timeline ── */}
            <div className="debug-sidebar__section-title">
                TTD Timeline
                {ttd && <span className="debug-sidebar__ttd-count">&nbsp;({ttd.snapshot_count} steps)</span>}
            </div>
            <div className="debug-sidebar__ttd-controls">
                <button
                    className="debug-sidebar__btn debug-sidebar__btn--ttd"
                    onClick={ttd?.is_recording ? doTtdStop : doTtdStart}
                    disabled={ttdBusy}
                >
                    {ttd?.is_recording ? "⏹ Stop" : "⏺ Record"}
                </button>
            </div>
            {ttd && ttd.step_range && !ttd.is_recording && (
                <>
                    <div className="debug-sidebar__ttd-seek">
                        <button
                            className="debug-sidebar__btn debug-sidebar__btn--icon"
                            onClick={() => doTtdStep("rewind")}
                            disabled={ttdBusy}
                            title="Step back"
                        >⏮</button>
                        <input
                            className="debug-sidebar__input debug-sidebar__input--step"
                            type="number"
                            placeholder={String(ttd.current_step ?? ttd.step_range[0])}
                            value={ttdSeekInput}
                            onChange={(e) => setTtdSeekInput(e.target.value)}
                            onKeyDown={(e) => e.key === "Enter" && doTtdSeek()}
                            min={ttd.step_range[0]}
                            max={ttd.step_range[1]}
                        />
                        <button
                            className="debug-sidebar__btn debug-sidebar__btn--icon"
                            onClick={() => doTtdStep("forward")}
                            disabled={ttdBusy}
                            title="Step forward"
                        >⏭</button>
                    </div>
                    <div className="debug-sidebar__ttd-range">
                        Step {ttd.step_range[0]} – {ttd.step_range[1]}
                        {ttd.current_step !== null && ` · @ ${ttd.current_step}`}
                    </div>
                    {ttd.current_snapshot && (
                        <div className="debug-sidebar__ttd-regs">
                            <span>RIP: {ttd.current_snapshot.rip}</span>
                            <span>RAX: 0x{ttd.current_snapshot.rax.toString(16)}</span>
                            <span>RSP: 0x{ttd.current_snapshot.rsp.toString(16)}</span>
                        </div>
                    )}
                </>
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
