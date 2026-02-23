// Fission — String cross-references panel.
// Lets the user search for strings in the binary and see which code locations
// reference them, using the `get_string_xrefs` Tauri command.
//
// Search syntax:
//   plain text   — case-insensitive substring match (sent to backend)
//   "exact"      — exact string match (double-quoted, client-side filter)
//   /pattern/    — JavaScript regex match (client-side filter)

import { useState, useCallback, useMemo, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { invoke } from "@tauri-apps/api/core";
import type { StringXrefDto } from "../types";

interface StringXrefsPanelProps {
    binaryLoaded: boolean;
    onLog: (msg: string) => void;
    /** Optional: navigate the editor to this address */
    onAddressClick?: (address: string) => void;
}

/** Parse search mode from the raw query string. */
function parseQuery(raw: string): { backendQuery: string; clientFilter: ((s: string) => boolean) | null } {
    const trimmed = raw.trim();
    // Exact: "foo bar"
    if (trimmed.startsWith('"') && trimmed.endsWith('"') && trimmed.length > 2) {
        const exact = trimmed.slice(1, -1);
        return { backendQuery: exact, clientFilter: (s) => s === exact };
    }
    // Regex: /foo.*/
    if (trimmed.startsWith('/') && trimmed.lastIndexOf('/') > 0) {
        const last = trimmed.lastIndexOf('/');
        const pattern = trimmed.slice(1, last);
        const flags = trimmed.slice(last + 1) || 'i';
        try {
            const re = new RegExp(pattern, flags);
            return { backendQuery: '', clientFilter: (s) => re.test(s) };
        } catch {
            /* invalid regex — fall through to plain */
        }
    }
    return { backendQuery: trimmed, clientFilter: null };
}

export function StringXrefsPanel({
    binaryLoaded,
    onLog,
    onAddressClick,
}: StringXrefsPanelProps) {
    const [query, setQuery] = useState("");
    const [minLength, setMinLength] = useState(4);
    const [rawResults, setRawResults] = useState<StringXrefDto[]>([]);
    const [loading, setLoading] = useState(false);
    const [expanded, setExpanded] = useState<Set<string>>(new Set());

    const handleSearch = useCallback(async () => {
        if (!binaryLoaded) {
            onLog("No binary loaded.");
            return;
        }
        setLoading(true);
        setRawResults([]);
        setExpanded(new Set());
        const { backendQuery } = parseQuery(query);
        try {
            const res = await invoke<StringXrefDto[]>("get_string_xrefs", {
                search: backendQuery,
                minLength,
            });
            setRawResults(res);
            onLog(
                `String XRefs: fetched ${res.length} result(s) for "${query || "*"}"`
            );
        } catch (err) {
            onLog(`String XRefs error: ${err}`);
        } finally {
            setLoading(false);
        }
    }, [binaryLoaded, query, minLength, onLog]);

    /** Client-side filtered results (regex / exact) */
    const results = useMemo(() => {
        const { clientFilter } = parseQuery(query);
        if (!clientFilter) return rawResults;
        return rawResults.filter((r) => clientFilter(r.string_value));
    }, [rawResults, query]);

    const toggleExpand = useCallback((addr: string) => {
        setExpanded((prev) => {
            const next = new Set(prev);
            if (next.has(addr)) {
                next.delete(addr);
            } else {
                next.add(addr);
            }
            return next;
        });
    }, []);

    const scrollRef = useRef<HTMLDivElement>(null);

    const rowVirtualizer = useVirtualizer({
        count: results.length,
        getScrollElement: () => scrollRef.current,
        estimateSize: () => 32, // collapsed header height
        overscan: 10,
        measureElement: (el) => el.getBoundingClientRect().height,
    });

    return (
        <div className="string-xrefs-panel">
            {/* Search bar */}
            <div className="string-xrefs-panel__toolbar">
                <input
                    className="string-xrefs-panel__input"
                    type="text"
                    placeholder='text · "exact" · /regex/'
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                    onKeyDown={(e) => e.key === "Enter" && handleSearch()}
                    disabled={!binaryLoaded || loading}
                    title='Search syntax: plain text (substring), "exact" (exact match), /pattern/ (regex)'
                />
                <label className="string-xrefs-panel__label">
                    Min length:
                    <input
                        className="string-xrefs-panel__minlen"
                        type="number"
                        min={2}
                        max={64}
                        value={minLength}
                        onChange={(e) => setMinLength(Number(e.target.value))}
                        disabled={!binaryLoaded || loading}
                    />
                </label>
                <button
                    className="string-xrefs-panel__btn"
                    onClick={handleSearch}
                    disabled={!binaryLoaded || loading}
                >
                    {loading ? "Scanning…" : "Scan"}
                </button>
            </div>

            {results.length === 0 && !loading && (
                <div className="string-xrefs-panel__empty">
                    {binaryLoaded
                        ? 'Enter a search term and press "Scan".'
                        : "Open a binary to use this panel."}
                </div>
            )}

            {results.length > 0 && (
                <div ref={scrollRef} className="string-xrefs-panel__results">
                    <div style={{ height: rowVirtualizer.getTotalSize(), position: "relative" }}>
                        {rowVirtualizer.getVirtualItems().map((vRow) => {
                            const item = results[vRow.index];
                            const isOpen = expanded.has(item.string_address);
                            return (
                                <div
                                    key={vRow.key}
                                    data-index={vRow.index}
                                    ref={rowVirtualizer.measureElement}
                                    style={{ position: "absolute", top: 0, left: 0, width: "100%", transform: `translateY(${vRow.start}px)` }}
                                >
                                    <div className="sxref-item">
                                        {/* String header row */}
                                        <div
                                            className="sxref-item__header"
                                            onClick={() => toggleExpand(item.string_address)}
                                        >
                                            <span className={`sxref-item__arrow ${isOpen ? "sxref-item__arrow--open" : ""}`}>
                                                ▶
                                            </span>
                                            <span
                                                className="sxref-item__addr"
                                                title="Go to address"
                                                onClick={(e) => {
                                                    e.stopPropagation();
                                                    onAddressClick?.(item.string_address);
                                                }}
                                            >
                                                {item.string_address}
                                            </span>
                                            <span className="sxref-item__value">{item.string_value}</span>
                                            <span className="sxref-item__count">
                                                {item.refs.length} ref{item.refs.length !== 1 ? "s" : ""}
                                            </span>
                                        </div>

                                        {/* Callsite rows */}
                                        {isOpen && item.refs.length > 0 && (
                                            <table className="sxref-item__table">
                                                <thead>
                                                    <tr>
                                                        <th>From Address</th>
                                                        <th>Function</th>
                                                    </tr>
                                                </thead>
                                                <tbody>
                                                    {item.refs.map((cs, i) => (
                                                        <tr
                                                            key={i}
                                                            className="sxref-item__cs-row data-table__row"
                                                            onClick={() => onAddressClick?.(cs.from_address)}
                                                        >
                                                            <td className="data-table__addr">{cs.from_address}</td>
                                                            <td>{cs.from_function ?? "—"}</td>
                                                        </tr>
                                                    ))}
                                                </tbody>
                                            </table>
                                        )}
                                        {isOpen && item.refs.length === 0 && (
                                            <div className="sxref-item__no-refs">No code references found.</div>
                                        )}
                                    </div>
                                </div>
                            );
                        })}
                    </div>
                </div>
            )}
        </div>
    );
}
