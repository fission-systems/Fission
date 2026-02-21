import { useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { SearchResultDto } from "../types";

interface SearchPanelProps {
    onResultClick?: (address: string) => void;
    binaryLoaded: boolean;
}

export default function SearchPanel({ onResultClick, binaryLoaded }: SearchPanelProps) {
    const [query, setQuery] = useState("");
    const [results, setResults] = useState<SearchResultDto[]>([]);
    const [searching, setSearching] = useState(false);
    const [searchedQuery, setSearchedQuery] = useState("");

    const handleSearch = useCallback(async () => {
        if (!query.trim() || !binaryLoaded) return;
        setSearching(true);
        try {
            const res = await invoke<SearchResultDto[]>("search_binary", { query: query.trim() });
            setResults(res);
            setSearchedQuery(query.trim());
        } catch (err) {
            console.error("Search error:", err);
            setResults([]);
        } finally {
            setSearching(false);
        }
    }, [query, binaryLoaded]);

    const typeIcon = (type: string) => {
        switch (type) {
            case "function": return "ƒ";
            case "string": return "\"";
            case "address": return "#";
            default: return "?";
        }
    };

    const typeClass = (type: string) => {
        switch (type) {
            case "function": return "search-result--func";
            case "string": return "search-result--str";
            case "address": return "search-result--addr";
            default: return "";
        }
    };

    return (
        <div className="search-panel">
            <div className="search-panel__input-row">
                <input
                    className="search-panel__input"
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                    onKeyDown={(e) => e.key === "Enter" && handleSearch()}
                    placeholder={binaryLoaded ? "Search functions, strings, addresses..." : "Load a binary first"}
                    disabled={!binaryLoaded}
                    spellCheck={false}
                />
                <button
                    className="search-panel__btn"
                    onClick={handleSearch}
                    disabled={!binaryLoaded || searching}
                >
                    {searching ? "…" : "Search"}
                </button>
            </div>

            {searchedQuery && (
                <div className="search-panel__status">
                    {results.length} result{results.length !== 1 ? "s" : ""} for "{searchedQuery}"
                </div>
            )}

            <div className="search-panel__results">
                {results.map((r, i) => (
                    <div
                        key={i}
                        className={`search-result ${typeClass(r.result_type)}`}
                        onClick={() => onResultClick?.(r.address)}
                    >
                        <span className="search-result__icon">{typeIcon(r.result_type)}</span>
                        <div className="search-result__info">
                            <div className="search-result__name">{r.name}</div>
                            <div className="search-result__meta">
                                <span className="search-result__addr">{r.address}</span>
                                <span className="search-result__ctx">{r.context}</span>
                            </div>
                        </div>
                    </div>
                ))}
            </div>
        </div>
    );
}
