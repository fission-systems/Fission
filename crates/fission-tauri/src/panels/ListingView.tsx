import React, { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ListingInfo, ListingRow } from "../types";

interface Props {
    binaryLoaded: boolean;
    onLog: (msg: string) => void;
}

const CHUNK_SIZE = 300;

export const ListingView: React.FC<Props> = ({ binaryLoaded, onLog }) => {
    const [info, setInfo] = useState<ListingInfo | null>(null);
    const [rows, setRows] = useState<ListingRow[]>([]);
    const [loading, setLoading] = useState(false);
    const [exhausted, setExhausted] = useState(false);
    const [jumpInput, setJumpInput] = useState("");
    const [error, setError] = useState<string | null>(null);

    // Address of the next chunk to load (track per-instruction, avoiding label rows)
    const nextAddrRef = useRef<string | null>(null);
    const sentinelRef = useRef<HTMLDivElement>(null);
    const containerRef = useRef<HTMLDivElement>(null);

    // -------------------------------------------------------------------------
    // Load a chunk of rows from backend
    // -------------------------------------------------------------------------
    const loadChunk = useCallback(async (startAddress: string, reset = false) => {
        if (loading) return;
        setLoading(true);

        try {
            const chunk = await invoke<ListingRow[]>("get_listing_chunk", {
                startAddress,
                count: CHUNK_SIZE,
            });

            if (chunk.length === 0) {
                setExhausted(true);
            } else {
                // Find the last instruction row to track next fetch address
                const lastInsn = [...chunk].reverse().find((r) => r.row_type === "instruction");
                if (lastInsn) {
                    // Next chunk starts one byte after the last instruction — we'll decode
                    // from there and the decoder will align naturally.
                    // We pass the address of the last instruction + 1 byte as hint.
                    // Backend finds the section that contains it.
                    const lastAddr = BigInt(lastInsn.address);
                    nextAddrRef.current = `0x${(lastAddr + 1n).toString(16)}`;
                } else {
                    setExhausted(true);
                }

                if (reset) {
                    setRows(chunk);
                } else {
                    setRows((prev) => [...prev, ...chunk]);
                }
            }
        } catch (e) {
            const msg = String(e);
            onLog(`[Listing] Chunk load error: ${msg}`);
        } finally {
            setLoading(false);
        }
    }, [loading, onLog]);

    // -------------------------------------------------------------------------
    // Bootstrap: fetch listing info, then load first chunk.
    // bootstrapRef lets the binaryLoaded effect always call the latest version
    // without depending on bootstrap (which changes whenever loading flips).
    // -------------------------------------------------------------------------
    const bootstrapRef = useRef<(startAddr?: string) => Promise<void>>(async () => {});

    const bootstrap = useCallback(async (startAddr?: string) => {
        setError(null);
        setRows([]);
        setExhausted(false);
        nextAddrRef.current = null;

        try {
            const meta = await invoke<ListingInfo>("get_listing_info");
            setInfo(meta);

            const resolvedStart = startAddr ?? meta.first_addr;
            await loadChunk(resolvedStart, true /* reset */);
        } catch (e) {
            const msg = String(e);
            setError(msg);
            onLog(`[Listing] Error: ${msg}`);
        }
    }, [loadChunk, onLog]);

    // Keep the ref current so binaryLoaded effect always calls the latest version
    useEffect(() => { bootstrapRef.current = bootstrap; }, [bootstrap]);

    // -------------------------------------------------------------------------
    // IntersectionObserver: load more when sentinel is visible
    // -------------------------------------------------------------------------
    useEffect(() => {
        const sentinel = sentinelRef.current;
        if (!sentinel) return;

        const observer = new IntersectionObserver(
            (entries) => {
                if (entries[0].isIntersecting && !loading && !exhausted && nextAddrRef.current) {
                    loadChunk(nextAddrRef.current);
                }
            },
            { threshold: 0.1 }
        );

        observer.observe(sentinel);
        return () => observer.disconnect();
    }, [loading, exhausted, loadChunk]);

    // -------------------------------------------------------------------------
    // Re-bootstrap when binary is loaded/unloaded
    // -------------------------------------------------------------------------
    useEffect(() => {
        if (binaryLoaded) {
            bootstrapRef.current();
        } else {
            setInfo(null);
            setRows([]);
            setError(null);
            setExhausted(false);
            nextAddrRef.current = null;
        }
    }, [binaryLoaded]);

    // -------------------------------------------------------------------------
    // Jump to address
    // -------------------------------------------------------------------------
    const handleJump = async () => {
        const trimmed = jumpInput.trim();
        if (!trimmed) return;

        const addr = trimmed.startsWith("0x") || trimmed.startsWith("0X")
            ? trimmed
            : `0x${trimmed}`;

        try {
            BigInt(addr); // validate
        } catch {
            onLog(`[Listing] Invalid address: ${trimmed}`);
            return;
        }

        await bootstrap(addr);
        containerRef.current?.scrollTo({ top: 0 });
    };

    // -------------------------------------------------------------------------
    // Render
    // -------------------------------------------------------------------------
    if (!binaryLoaded) {
        return (
            <div className="listing-empty">
                <span className="listing-empty__icon">📄</span>
                <p>No binary loaded</p>
            </div>
        );
    }

    if (error) {
        return (
            <div className="listing-empty listing-empty--error">
                <span className="listing-empty__icon">⚠</span>
                <p>{error}</p>
            </div>
        );
    }

    return (
        <div className="listing-view">
            {/* Toolbar */}
            <div className="listing-view__toolbar">
                {info && (
                    <span className="listing-view__meta">
                        EP: {info.entry_point} | {info.first_addr} – {info.last_addr}
                        &nbsp;({(Number(info.total_exec_bytes) / 1024).toFixed(1)} KB executable)
                    </span>
                )}
                <div className="listing-view__jump">
                    <input
                        className="listing-view__jump-input"
                        placeholder="Jump to address…"
                        value={jumpInput}
                        onChange={(e) => setJumpInput(e.target.value)}
                        onKeyDown={(e) => e.key === "Enter" && handleJump()}
                    />
                    <button className="listing-view__jump-btn" onClick={handleJump}>
                        Go
                    </button>
                </div>
            </div>

            {/* Row list */}
            <div className="listing-view__body" ref={containerRef}>
                <table className="listing-view__table">
                    <tbody>
                        {rows.map((row, i) =>
                            row.row_type === "label" ? (
                                <tr key={`lbl-${i}`} className="listing-row listing-row--label">
                                    <td colSpan={4} className="listing-row__label">
                                        {row.label}:
                                    </td>
                                </tr>
                            ) : (
                                <tr key={`ins-${i}`} className="listing-row listing-row--insn">
                                    <td className="listing-row__addr">{row.address}</td>
                                    <td className="listing-row__bytes">{row.bytes}</td>
                                    <td className="listing-row__mnem">{row.mnemonic}</td>
                                    <td className="listing-row__ops">
                                        {row.operands}
                                        {row.comment && (
                                            <span className="listing-row__comment">
                                                &nbsp;&nbsp;; {row.comment}
                                            </span>
                                        )}
                                    </td>
                                </tr>
                            )
                        )}
                    </tbody>
                </table>

                {/* Infinite-scroll sentinel */}
                <div ref={sentinelRef} className="listing-view__sentinel">
                    {loading && <span className="listing-view__loading">Loading…</span>}
                    {exhausted && !loading && (
                        <span className="listing-view__end">— end of section —</span>
                    )}
                </div>
            </div>
        </div>
    );
};

export default ListingView;
