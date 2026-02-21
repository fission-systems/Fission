// Fission — String cross-references panel.
// Lets the user search for strings in the binary and see which code locations
// reference them, using the `get_string_xrefs` Tauri command.

import { useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { StringXrefDto } from "../types";

interface StringXrefsPanelProps {
    binaryLoaded: boolean;
    onLog: (msg: string) => void;
    /** Optional: navigate the editor to this address */
    onAddressClick?: (address: string) => void;
}

export function StringXrefsPanel({
    binaryLoaded,
    onLog,
    onAddressClick,
}: StringXrefsPanelProps) {
    const [query, setQuery] = useState("");
    const [minLength, setMinLength] = useState(4);
    const [results, setResults] = useState<StringXrefDto[]>([]);
    const [loading, setLoading] = useState(false);
    const [expanded, setExpanded] = useState<Set<string>>(new Set());

    const handleSearch = useCallback(async () => {
        if (!binaryLoaded) {
            onLog("No binary loaded.");
            return;
        }
        setLoading(true);
        setResults([]);
        setExpanded(new Set());
        try {
            const res = await invoke<StringXrefDto[]>("get_string_xrefs", {
                search: query,
                minLength,
            });
            setResults(res);
            onLog(
                `String XRefs: found ${res.length} string(s) matching "${query || "*"}" (${res.reduce((s, r) => s + r.refs.length, 0)} total refs)`
            );
        } catch (err) {
            onLog(`String XRefs error: ${err}`);
        } finally {
            setLoading(false);
        }
    }, [binaryLoaded, query, minLength, onLog]);

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

    return (
        <div className="string-xrefs-panel">
            {/* Search bar */}
            <div className="string-xrefs-panel__toolbar">
                <input
                    className="string-xrefs-panel__input"
                    type="text"
                    placeholder="Search strings… (empty = all)"
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                    onKeyDown={(e) => e.key === "Enter" && handleSearch()}
                    disabled={!binaryLoaded || loading}
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

            {/* Results */}
            {results.length === 0 && !loading && (
                <div className="string-xrefs-panel__empty">
                    {binaryLoaded
                        ? 'Enter a search term and press "Scan".'
                        : "Open a binary to use this panel."}
                </div>
            )}

            {results.length > 0 && (
                <div className="string-xrefs-panel__results">
                    {results.map((item) => {
                        const isOpen = expanded.has(item.string_address);
                        return (
                            <div key={item.string_address} className="sxref-item">
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
                        );
                    })}
                </div>
            )}
        </div>
    );
}
