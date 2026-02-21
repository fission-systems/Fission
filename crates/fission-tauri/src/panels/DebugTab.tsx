import React, { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { BreakpointInfoDto, DebugStateDto, DebugStatusDto, RegisterStateDto } from "../types";

interface Props {
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

const REG_NAMES: (keyof RegisterStateDto)[] = [
    "rax", "rbx", "rcx", "rdx",
    "rsi", "rdi", "rbp", "rsp",
    "r8",  "r9",  "r10", "r11",
    "r12", "r13", "r14", "r15",
    "rip", "rflags",
];

export const DebugTab: React.FC<Props> = ({ onLog }) => {
    const [ds, setDs] = useState<DebugStateDto>({
        status: "detached",
        attached_pid: null,
        breakpoints: [],
        registers: null,
        last_event: null,
        events: [],
    });
    const [pidInput, setPidInput] = useState("");
    const [bpInput, setBpInput] = useState("");
    const [busy, setBusy] = useState(false);
    const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);
    const eventsRef = useRef<HTMLDivElement>(null);

    // -------------------------------------------------------------------------
    // Poll debug state every second when attached
    // -------------------------------------------------------------------------
    const fetchState = useCallback(async () => {
        try {
            const state = await invoke<DebugStateDto>("debug_get_state");
            setDs(state);
        } catch {
            // non-critical
        }
    }, []);

    useEffect(() => {
        fetchState();
    }, []); // eslint-disable-line react-hooks/exhaustive-deps

    useEffect(() => {
        if (ds.status !== "detached" && ds.status !== "terminated") {
            pollRef.current = setInterval(fetchState, 1000);
        } else {
            if (pollRef.current) clearInterval(pollRef.current);
        }
        return () => { if (pollRef.current) clearInterval(pollRef.current); };
    }, [ds.status, fetchState]);

    // Auto-scroll events log
    useEffect(() => {
        const el = eventsRef.current;
        if (el) el.scrollTop = el.scrollHeight;
    }, [ds.events.length]);

    // -------------------------------------------------------------------------
    // Actions
    // -------------------------------------------------------------------------
    const doAttach = async () => {
        const pid = parseInt(pidInput.trim(), 10);
        if (isNaN(pid) || pid <= 0) return;
        setBusy(true);
        try {
            await invoke("debug_attach", { pid });
            onLog(`[Debug] Attached to PID ${pid}`);
            await fetchState();
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

    const doContinue = async () => {
        setBusy(true);
        try {
            await invoke("debug_continue");
            await fetchState();
        } catch (e) {
            onLog(`[Debug] Continue failed: ${e}`);
        } finally {
            setBusy(false);
        }
    };

    const doStep = async () => {
        setBusy(true);
        try {
            await invoke("debug_step");
            await fetchState();
        } catch (e) {
            onLog(`[Debug] Step failed: ${e}`);
        } finally {
            setBusy(false);
        }
    };

    const doAddBp = async () => {
        const trimmed = bpInput.trim();
        if (!trimmed) return;
        const normalized = trimmed.startsWith("0x") || trimmed.startsWith("0X") ? trimmed : "0x" + trimmed;
        const addr = parseInt(normalized, 16);
        if (isNaN(addr)) { onLog("[Debug] Invalid address"); return; }
        setBusy(true);
        try {
            await invoke("debug_add_breakpoint", { address: addr });
            setBpInput("");
            await fetchState();
        } catch (e) {
            onLog(`[Debug] Add breakpoint failed: ${e}`);
        } finally {
            setBusy(false);
        }
    };

    const doRemoveBp = async (bp: BreakpointInfoDto) => {
        const addr = parseInt(bp.address, 16);
        setBusy(true);
        try {
            await invoke("debug_remove_breakpoint", { address: addr });
            await fetchState();
        } catch (e) {
            onLog(`[Debug] Remove breakpoint failed: ${e}`);
        } finally {
            setBusy(false);
        }
    };

    const isAttached = ds.attached_pid !== null;
    const isSuspended = ds.status === "suspended";
    const statusColor = STATUS_COLORS[ds.status];

    // -------------------------------------------------------------------------
    // Render
    // -------------------------------------------------------------------------
    return (
        <div className="debug-tab">
            {/* ── Control bar ──────────────────────────────────────────── */}
            <div className="debug-tab__bar">
                {/* Status badge */}
                <span className="debug-tab__status" style={{ color: statusColor }}>
                    {STATUS_ICONS[ds.status]} {ds.status.charAt(0).toUpperCase() + ds.status.slice(1)}
                </span>

                {isAttached && (
                    <span className="debug-tab__pid">PID: {ds.attached_pid}</span>
                )}

                {ds.last_event && (
                    <span className="debug-tab__last-event">{ds.last_event}</span>
                )}

                <span className="debug-tab__spacer" />

                {!isAttached && (
                    <>
                        <input
                            className="debug-tab__input"
                            placeholder="PID"
                            value={pidInput}
                            onChange={(e) => setPidInput(e.target.value)}
                            onKeyDown={(e) => e.key === "Enter" && doAttach()}
                            style={{ width: 72 }}
                        />
                        <button
                            className="debug-tab__btn debug-tab__btn--attach"
                            onClick={doAttach}
                            disabled={busy || !pidInput.trim()}
                        >
                            🔗 Attach
                        </button>
                    </>
                )}

                {isAttached && (
                    <>
                        <button
                            className="debug-tab__btn"
                            onClick={doContinue}
                            disabled={busy || !isSuspended}
                            title="Continue (F5)"
                        >
                            ▶ Continue
                        </button>
                        <button
                            className="debug-tab__btn"
                            onClick={doStep}
                            disabled={busy || !isSuspended}
                            title="Single step (F11)"
                        >
                            ⏭ Step
                        </button>
                        <button
                            className="debug-tab__btn debug-tab__btn--detach"
                            onClick={doDetach}
                            disabled={busy}
                        >
                            ⏏ Detach
                        </button>
                    </>
                )}
            </div>

            {/* ── Body ─────────────────────────────────────────────────── */}
            <div className="debug-tab__body">
                {/* Registers */}
                <div className="debug-tab__section">
                    <div className="debug-tab__section-title">Registers</div>
                    {ds.registers ? (
                        <table className="debug-tab__reg-table">
                            <tbody>
                                {REG_NAMES.map((r) => (
                                    <tr key={r}>
                                        <td className="debug-tab__reg-name">{r}</td>
                                        <td className="debug-tab__reg-val">
                                            0x{(ds.registers![r] >>> 0).toString(16).padStart(16, "0")}
                                        </td>
                                    </tr>
                                ))}
                            </tbody>
                        </table>
                    ) : (
                        <div className="debug-tab__empty">No register data</div>
                    )}
                </div>

                {/* Breakpoints */}
                <div className="debug-tab__section">
                    <div className="debug-tab__section-title">Breakpoints</div>
                    <div className="debug-tab__bp-add">
                        <input
                            className="debug-tab__input"
                            placeholder="0xADDR"
                            value={bpInput}
                            onChange={(e) => setBpInput(e.target.value)}
                            onKeyDown={(e) => e.key === "Enter" && doAddBp()}
                        />
                        <button
                            className="debug-tab__btn"
                            onClick={doAddBp}
                            disabled={busy || !bpInput.trim()}
                        >
                            + Add
                        </button>
                    </div>
                    {ds.breakpoints.length === 0 ? (
                        <div className="debug-tab__empty">No breakpoints</div>
                    ) : (
                        <table className="debug-tab__bp-table">
                            <tbody>
                                {ds.breakpoints.map((bp) => (
                                    <tr key={bp.address} className="debug-tab__bp-row">
                                        <td className="debug-tab__bp-addr">{bp.address}</td>
                                        <td className="debug-tab__bp-en">
                                            {bp.enabled ? "●" : "○"}
                                        </td>
                                        <td>
                                            <button
                                                className="debug-tab__bp-del"
                                                onClick={() => doRemoveBp(bp)}
                                                title="Remove breakpoint"
                                            >
                                                ✕
                                            </button>
                                        </td>
                                    </tr>
                                ))}
                            </tbody>
                        </table>
                    )}
                </div>

                {/* Events log */}
                <div className="debug-tab__section debug-tab__section--events">
                    <div className="debug-tab__section-title">Events</div>
                    <div className="debug-tab__events" ref={eventsRef}>
                        {ds.events.length === 0 ? (
                            <div className="debug-tab__empty">No events yet</div>
                        ) : (
                            ds.events.map((ev, i) => (
                                <div key={i} className="debug-tab__event">{ev}</div>
                            ))
                        )}
                    </div>
                </div>
            </div>
        </div>
    );
};

export default DebugTab;
