import { useState, useMemo } from "react";
import type { FunctionDto } from "../types";

type CategoryFilter = "all" | "import" | "export" | "internal";

interface FunctionsListProps {
    functions: FunctionDto[];
    loading: boolean;
    onFunctionClick: (func: FunctionDto) => void;
    onOpenFile: () => void;
    selectedAddress: string | null;
    onAnalyze?: () => void;
    onDeepScan?: () => void;
    analyzing?: boolean;
    deepScanning?: boolean;
}

export default function FunctionsList({
    functions,
    loading,
    onFunctionClick,
    onOpenFile,
    selectedAddress,
    onAnalyze,
    onDeepScan,
    analyzing = false,
    deepScanning = false,
}: FunctionsListProps) {
    const [filter, setFilter] = useState("");
    const [category, setCategory] = useState<CategoryFilter>("all");

    const counts = useMemo(() => ({
        all: functions.length,
        import: functions.filter((f) => f.category === "import").length,
        export: functions.filter((f) => f.category === "export").length,
        internal: functions.filter((f) => f.category === "internal").length,
    }), [functions]);

    const filtered = useMemo(() => {
        let list = functions;
        if (category !== "all") list = list.filter((f) => f.category === category);
        if (filter) {
            const lc = filter.toLowerCase();
            list = list.filter(
                (f) =>
                    f.name.toLowerCase().includes(lc) ||
                    f.address.toLowerCase().includes(lc),
            );
        }
        return list;
    }, [functions, filter, category]);

    if (functions.length === 0 && !loading) {
        return (
            <div className="functions-list">
                <div className="functions-list__open-btn" onClick={onOpenFile}>
                    📂 Open Binary File
                </div>
            </div>
        );
    }

    return (
        <div className="functions-list">
            {/* Analyse toolbar */}
            {functions.length > 0 && (
                <div className="functions-list__toolbar">
                    <button
                        className="functions-list__tool-btn"
                        title="Analyze: discover internal functions via CALL targets"
                        onClick={onAnalyze}
                        disabled={analyzing || deepScanning}
                    >
                        {analyzing ? "…" : "🔍"} Analyze
                    </button>
                    <button
                        className="functions-list__tool-btn"
                        title="Deep Scan: discover functions via prologue pattern matching"
                        onClick={onDeepScan}
                        disabled={analyzing || deepScanning}
                    >
                        {deepScanning ? "…" : "🕵"} Deep Scan
                    </button>
                </div>
            )}

            {/* Category filter */}
            {functions.length > 0 && (
                <div className="functions-list__cats">
                    {(["all", "import", "export", "internal"] as CategoryFilter[]).map((cat) => (
                        <button
                            key={cat}
                            className={`functions-list__cat-btn ${category === cat ? "functions-list__cat-btn--active" : ""}`}
                            onClick={() => setCategory(cat)}
                        >
                            {cat === "all" ? "All" : cat === "import" ? "Imp" : cat === "export" ? "Exp" : "Int"}
                            <span className="functions-list__cat-count">{counts[cat]}</span>
                        </button>
                    ))}
                </div>
            )}

            {/* Search */}
            <div className="functions-list__search">
                <input
                    type="text"
                    placeholder="Filter functions..."
                    value={filter}
                    onChange={(e) => setFilter(e.target.value)}
                    spellCheck={false}
                />
            </div>

            <div className="functions-list__items">
                {loading && <div style={{ padding: 12, color: "var(--text-muted)" }}>Loading...</div>}
                {filtered.map((f) => (
                    <div
                        key={f.address}
                        className={`functions-list__item ${selectedAddress === f.address ? "functions-list__item--selected" : ""}`}
                        onClick={() => onFunctionClick(f)}
                    >
                        <span className="functions-list__item-addr">{f.address}</span>
                        <span className="functions-list__item-name">{f.name}</span>
                        {f.category === "import" && (
                            <span className="functions-list__item-badge functions-list__item-badge--import">IMP</span>
                        )}
                        {f.category === "export" && (
                            <span className="functions-list__item-badge functions-list__item-badge--export">EXP</span>
                        )}
                    </div>
                ))}
            </div>
        </div>
    );
}

