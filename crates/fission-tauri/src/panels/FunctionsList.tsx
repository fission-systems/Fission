import { useState, useMemo } from "react";
import type { FunctionDto } from "../types";

interface Props {
    functions: FunctionDto[];
    loading: boolean;
    onFunctionClick: (func: FunctionDto) => void;
    onOpenFile: () => void;
    selectedAddress: string | null;
}

export default function FunctionsList({
    functions,
    loading,
    onFunctionClick,
    onOpenFile,
    selectedAddress,
}: Props) {
    const [filter, setFilter] = useState("");

    const filtered = useMemo(() => {
        if (!filter) return functions;
        const lower = filter.toLowerCase();
        return functions.filter(
            (f) =>
                f.name.toLowerCase().includes(lower) ||
                f.address.toLowerCase().includes(lower),
        );
    }, [functions, filter]);

    if (loading) {
        return (
            <div className="loading">
                <div className="spinner" />
                Loading...
            </div>
        );
    }

    if (functions.length === 0) {
        return (
            <div className="empty-state">
                <div>No binary loaded</div>
                <button className="welcome__action" onClick={onOpenFile} style={{ fontSize: 12, padding: "4px 16px" }}>
                    Open File
                </button>
            </div>
        );
    }

    return (
        <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
            <div className="function-list__search">
                <input
                    type="text"
                    placeholder="Filter functions... (name or address)"
                    value={filter}
                    onChange={(e) => setFilter(e.target.value)}
                />
            </div>

            <div className="function-list__items">
                {filtered.map((func) => (
                    <div
                        key={func.address}
                        className={`function-list__item ${func.address === selectedAddress ? "function-list__item--selected" : ""}`}
                        onClick={() => onFunctionClick(func)}
                    >
                        <span className="function-list__addr">{func.address}</span>
                        <span className="function-list__name">{func.name}</span>
                        {func.size > 0 && <span className="function-list__size">{func.size}B</span>}
                    </div>
                ))}
            </div>

            <div className="function-list__count">
                {filtered.length === functions.length
                    ? `${functions.length} functions`
                    : `${filtered.length} / ${functions.length} functions`}
            </div>
        </div>
    );
}
