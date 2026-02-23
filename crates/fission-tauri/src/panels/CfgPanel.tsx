/**
 * CfgPanel — Control Flow Graph visualiser for the selected function.
 *
 * Layout algorithm: simple BFS layering (top-down).
 * Renders nodes as SVG <rect>/<text> and edges as cubic Bezier curves.
 * Falls back gracefully when no binary / function is selected.
 */
import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { CfgDto, CfgNode, CfgEdge } from "../types";

interface CfgPanelProps {
    /** Hex address of the currently selected function (e.g. `"0x401000"`). */
    address: string | null;
    binaryLoaded: boolean;
    onLog: (msg: string) => void;
}

// Layout constants
const NODE_W = 240;
const LINE_H = 14;
const PAD = 8;
const H_GAP = 50;
const V_GAP = 55;

// ── helpers ────────────────────────────────────────────────────────────────

function nodeHeight(n: CfgNode): number {
    return PAD * 2 + LINE_H * (n.instructions.length + 1);
}

function edgeColor(kind: "unconditional" | "true" | "false" | string): string {
    return kind === "true" ? "#a6e3a1" : kind === "false" ? "#f38ba8" : "#7f849c";
}

/** BFS layers starting from any node marked `is_entry`, else node 0. */
function buildLayers(cfg: CfgDto): number[][] {
    const entryId = cfg.nodes.find((n) => n.is_entry)?.id ?? 0;
    const visited = new Set<number>();
    const layers: number[][] = [];
    let frontier = [entryId];

    while (frontier.length > 0) {
        layers.push(frontier);
        frontier.forEach((id) => visited.add(id));
        const next: number[] = [];
        for (const id of frontier) {
            for (const e of cfg.edges.filter((e) => e.from === id)) {
                if (!visited.has(e.to)) next.push(e.to);
            }
        }
        frontier = [...new Set(next)];
    }

    // Collect unreachable nodes in a final "layer"
    const unreachable = cfg.nodes.filter((n) => !visited.has(n.id)).map((n) => n.id);
    if (unreachable.length) layers.push(unreachable);

    return layers;
}

type PosMap = Record<number, { x: number; y: number; h: number }>;

function buildPositions(cfg: CfgDto, layers: number[][]): PosMap {
    const pos: PosMap = {};
    layers.forEach((layer, row) => {
        const totalW = layer.length * (NODE_W + H_GAP) - H_GAP;
        layer.forEach((id, col) => {
            const node = cfg.nodes.find((n) => n.id === id)!;
            pos[id] = {
                x: col * (NODE_W + H_GAP) - totalW / 2 + NODE_W / 2,
                y: row * (120 + V_GAP),
                h: nodeHeight(node),
            };
        });
    });
    return pos;
}

// ── component ──────────────────────────────────────────────────────────────

export default function CfgPanel({ address, binaryLoaded, onLog }: CfgPanelProps) {
    const [cfg, setCfg] = useState<CfgDto | null>(null);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const panelRef = useRef<HTMLDivElement>(null);
    const canvasRef = useRef<HTMLDivElement>(null);

    // Pan/zoom transform state
    const [scale, setScale] = useState(1.0);
    const [tx, setTx] = useState(0);
    const [ty, setTy] = useState(0);
    const dragRef = useRef<{ startX: number; startY: number; startTx: number; startTy: number } | null>(null);

    // Reset transform when a new function is loaded
    const resetTransform = useCallback(() => {
        setScale(1.0);
        setTx(0);
        setTy(0);
    }, []);

    const handleWheel = useCallback((e: React.WheelEvent<HTMLDivElement>) => {
        e.preventDefault();
        const rect = e.currentTarget.getBoundingClientRect();
        const mouseX = e.clientX - rect.left;
        const mouseY = e.clientY - rect.top;
        const factor = e.deltaY < 0 ? 1.1 : 1 / 1.1;
        setScale((s) => {
            const newScale = Math.min(5.0, Math.max(0.15, s * factor));
            const sd = newScale / s;
            setTx((t) => mouseX - sd * (mouseX - t));
            setTy((t) => mouseY - sd * (mouseY - t));
            return newScale;
        });
    }, []);

    const handlePointerDown = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
        if (e.button !== 0) return;
        e.currentTarget.setPointerCapture(e.pointerId);
        dragRef.current = { startX: e.clientX, startY: e.clientY, startTx: tx, startTy: ty };
    }, [tx, ty]);

    const handlePointerMove = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
        if (!dragRef.current) return;
        const dx = e.clientX - dragRef.current.startX;
        const dy = e.clientY - dragRef.current.startY;
        setTx(dragRef.current.startTx + dx);
        setTy(dragRef.current.startTy + dy);
    }, []);

    const handlePointerUp = useCallback(() => {
        dragRef.current = null;
    }, []);

    const fetchCfg = useCallback(
        async (addr: string) => {
            if (!addr) return;
            setLoading(true);
            setError(null);
            try {
                const result = await invoke<CfgDto>("get_cfg", { address: addr });
                setCfg(result);
            } catch (e) {
                const msg = String(e);
                onLog(`CFG error: ${msg}`);
                setError(msg);
                setCfg(null);
            } finally {
                setLoading(false);
            }
        },
        [onLog],
    );

    useEffect(() => {
        if (address && binaryLoaded) {
            resetTransform();
            fetchCfg(address);
        } else {
            setCfg(null);
            setError(null);
        }
    }, [address, binaryLoaded, fetchCfg, resetTransform]);

    const handleExportDot = useCallback(async () => {
        if (!address || !cfg) return;
        try {
            const dot = await invoke<string>("export_cfg_dot", { address });
            await navigator.clipboard.writeText(dot);
            onLog(`CFG DOT for ${cfg.function_name} copied to clipboard.`);
        } catch (e) {
            onLog(`Export DOT error: ${e}`);
        }
    }, [address, cfg, onLog]);

    // ── empty / loading states ──────────────────────────────────────────────
    if (!binaryLoaded || !address) {
        return (
            <div className="cfg-panel cfg-panel--empty">
                Select a function to view its control flow graph
            </div>
        );
    }

    if (loading) {
        return <div className="cfg-panel cfg-panel--empty">Analysing CFG…</div>;
    }

    if (error) {
        return (
            <div className="cfg-panel cfg-panel--empty cfg-panel--error">
                {error}
            </div>
        );
    }

    if (!cfg || cfg.nodes.length === 0) {
        return (
            <div className="cfg-panel cfg-panel--empty">
                No CFG data available for this function
            </div>
        );
    }

    // ── layout ─────────────────────────────────────────────────────────────
    const layers = buildLayers(cfg);
    const pos = buildPositions(cfg, layers);

    const allX = Object.values(pos).map((p) => p.x);
    const allY = Object.values(pos).map((p) => p.y + p.h);
    const minX = Math.min(...allX) - NODE_W / 2 - 24;
    const maxX = Math.max(...allX) + NODE_W / 2 + 24;
    const svgW = maxX - minX;
    const svgH = Math.max(...allY) + V_GAP + 24;

    return (
        <div className="cfg-panel" ref={panelRef}>
            {/* Toolbar */}
            <div className="cfg-panel__toolbar">
                <span className="cfg-panel__fn-name">{cfg.function_name}</span>
                <span className="cfg-panel__meta">
                    {cfg.block_count}&nbsp;blocks · {cfg.edge_count}&nbsp;edges · V(G)={cfg.cyclomatic_complexity}
                </span>
                <div className="cfg-panel__zoom-controls">
                    <button
                        className="cfg-panel__btn cfg-panel__btn--icon"
                        onClick={() => setScale((s) => Math.min(5.0, s * 1.2))}
                        title="Zoom in"
                    >＋</button>
                    <button
                        className="cfg-panel__btn cfg-panel__btn--icon cfg-panel__zoom-level"
                        onClick={resetTransform}
                        title="Reset zoom"
                    >{Math.round(scale * 100)}%</button>
                    <button
                        className="cfg-panel__btn cfg-panel__btn--icon"
                        onClick={() => setScale((s) => Math.max(0.15, s / 1.2))}
                        title="Zoom out"
                    >－</button>
                </div>
                <button
                    className="cfg-panel__btn"
                    onClick={() => fetchCfg(address)}
                    title="Refresh CFG"
                >
                    ↺
                </button>
                <button
                    className="cfg-panel__btn"
                    onClick={handleExportDot}
                    title="Copy Graphviz DOT to clipboard"
                >
                    Export DOT
                </button>
            </div>

            {/* SVG canvas — supports drag-to-pan and wheel-to-zoom */}
            <div
                ref={canvasRef}
                className="cfg-panel__canvas"
                onWheel={handleWheel}
                onPointerDown={handlePointerDown}
                onPointerMove={handlePointerMove}
                onPointerUp={handlePointerUp}
                onPointerLeave={handlePointerUp}
                style={{ cursor: dragRef.current ? "grabbing" : "grab" }}
            >
                <div
                    style={{
                        transform: `translate(${tx}px, ${ty}px) scale(${scale})`,
                        transformOrigin: "0 0",
                        willChange: "transform",
                    }}
                >
                <svg
                    width={svgW}
                    height={svgH}
                    viewBox={`${minX} -16 ${svgW} ${svgH + 16}`}
                    style={{ fontFamily: "monospace", fontSize: 11 }}
                >
                    <defs>
                        <marker
                            id="cfg-arrow"
                            markerWidth="8"
                            markerHeight="8"
                            refX="7"
                            refY="3"
                            orient="auto"
                        >
                            <path d="M0,0 L0,6 L8,3 z" fill="#7f849c" />
                        </marker>
                        <marker
                            id="cfg-arrow-true"
                            markerWidth="8"
                            markerHeight="8"
                            refX="7"
                            refY="3"
                            orient="auto"
                        >
                            <path d="M0,0 L0,6 L8,3 z" fill="#a6e3a1" />
                        </marker>
                        <marker
                            id="cfg-arrow-false"
                            markerWidth="8"
                            markerHeight="8"
                            refX="7"
                            refY="3"
                            orient="auto"
                        >
                            <path d="M0,0 L0,6 L8,3 z" fill="#f38ba8" />
                        </marker>
                    </defs>

                    {/* Edges (drawn under nodes) */}
                    {(cfg.edges as CfgEdge[]).map((edge, i) => {
                        const f = pos[edge.from];
                        const t = pos[edge.to];
                        if (!f || !t) return null;

                        // Source: bottom-centre; Target: top-centre
                        const sx = f.x;
                        const sy = f.y + f.h;
                        const tx = t.x;
                        const ty = t.y;
                        const cy = (sy + ty) / 2;

                        const markerId =
                            edge.kind === "true"
                                ? "cfg-arrow-true"
                                : edge.kind === "false"
                                  ? "cfg-arrow-false"
                                  : "cfg-arrow";

                        return (
                            <path
                                key={i}
                                d={`M${sx},${sy} C${sx},${cy} ${tx},${cy} ${tx},${ty}`}
                                fill="none"
                                stroke={edgeColor(edge.kind)}
                                strokeWidth={1.5}
                                markerEnd={`url(#${markerId})`}
                            />
                        );
                    })}

                    {/* Nodes */}
                    {(cfg.nodes as CfgNode[]).map((node) => {
                        const p = pos[node.id];
                        if (!p) return null;
                        const rx = p.x - NODE_W / 2;
                        const fill = node.is_entry
                            ? "#1e3a5f"
                            : node.is_exit
                              ? "#3d2e00"
                              : "#313244";
                        const stroke = node.is_entry
                            ? "#89b4fa"
                            : node.is_exit
                              ? "#f9e2af"
                              : "#45475a";

                        return (
                            <g key={node.id} transform={`translate(${rx},${p.y})`}>
                                <rect
                                    width={NODE_W}
                                    height={p.h}
                                    rx={5}
                                    fill={fill}
                                    stroke={stroke}
                                    strokeWidth={1}
                                />
                                {/* Address header */}
                                <text
                                    x={PAD}
                                    y={PAD + LINE_H - 2}
                                    fill="#89b4fa"
                                    fontWeight="bold"
                                    fontSize={10}
                                >
                                    {node.start_address}:
                                </text>
                                {/* Instructions */}
                                {node.instructions.map((ins, li) => (
                                    <text
                                        key={li}
                                        x={PAD}
                                        y={PAD + LINE_H * (li + 2) - 2}
                                        fill="#cdd6f4"
                                        fontSize={10}
                                    >
                                        {ins.length > 32 ? `${ins.slice(0, 31)}…` : ins}
                                    </text>
                                ))}
                            </g>
                        );
                    })}
                </svg>
                </div>{/* end transform wrapper */}
            </div>{/* end .cfg-panel__canvas */}
        </div>
    );
}
