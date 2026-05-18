import { useState, useMemo, useCallback, useEffect, useRef } from "react";
import type { FunctionCategory, FunctionDto } from "../../types";

type CategoryFilter = "all" | FunctionCategory;
type DecompileStatus = "idle" | "loading" | "ready" | "error";
type StatusFilter = "all" | DecompileStatus | "untouched";

interface FunctionsListProps {
    functions: FunctionDto[];
    loading: boolean;
    decompileStatus?: Record<string, DecompileStatus>;
    onFunctionClick: (func: FunctionDto) => void;
    onOpenFile: () => void;
    selectedAddress: string | null;
    onRenameFunc?: (func: FunctionDto) => void;
    onToggleBookmarkFunc?: (func: FunctionDto) => void;
    onCopyAddress?: (address: string) => void;
    onOpenHex?: (func: FunctionDto) => void;
}

export default function FunctionsList({
    functions,
    loading,
    decompileStatus = {},
    onFunctionClick,
    onOpenFile,
    selectedAddress,
    onRenameFunc,
    onToggleBookmarkFunc,
    onCopyAddress,
    onOpenHex,
}: FunctionsListProps) {
    const [filter, setFilter] = useState("");
    const [category, setCategory] = useState<CategoryFilter>("all");
    const [statusFilter, setStatusFilter] = useState<StatusFilter>("all");
    const [contextMenu, setContextMenu] = useState<{ x: number; y: number; func: FunctionDto } | null>(null);
    const contextMenuRef = useRef<HTMLDivElement>(null);

    const closeContextMenu = useCallback(() => setContextMenu(null), []);

    const handleContextMenu = useCallback((e: React.MouseEvent, func: FunctionDto) => {
        e.preventDefault();
        e.stopPropagation();
        setContextMenu({ x: e.clientX, y: e.clientY, func });
    }, []);

    // Close context menu on outside click or Escape key
    useEffect(() => {
        if (!contextMenu) return;
        const handleClick = () => closeContextMenu();
        const handleKey = (e: KeyboardEvent) => { if (e.key === "Escape") closeContextMenu(); };
        document.addEventListener("click", handleClick);
        document.addEventListener("keydown", handleKey);
        return () => {
            document.removeEventListener("click", handleClick);
            document.removeEventListener("keydown", handleKey);
        };
    }, [contextMenu, closeContextMenu]);

    const counts = useMemo(() => ({
        all: functions.length,
        import: functions.filter((f) => f.category === "import").length,
        export: functions.filter((f) => f.category === "export").length,
        internal: functions.filter((f) => f.category === "internal").length,
        thunk: functions.filter((f) => f.category === "thunk").length,
        external: functions.filter((f) => f.category === "external").length,
        debug: functions.filter((f) => f.category === "debug").length,
    }), [functions]);
    const summary = useMemo(() => ({
        internal: counts.internal,
        named: functions.filter((f) => !/^sub_[0-9a-f]+$/i.test(f.name)).length,
        imports: counts.import,
        exports: counts.export,
    }), [counts, functions]);
    const statusCounts = useMemo(() => {
        let loading = 0;
        let ready = 0;
        let error = 0;
        for (const func of functions) {
            const status = decompileStatus[`decomp-${func.address}`];
            if (status === "loading") loading++;
            if (status === "ready") ready++;
            if (status === "error") error++;
        }
        return {
            all: functions.length,
            ready,
            error,
            loading,
            untouched: Math.max(0, functions.length - ready - error - loading),
        };
    }, [decompileStatus, functions]);

    const functionStatus = useCallback(
        (func: FunctionDto): DecompileStatus | "untouched" =>
            decompileStatus[`decomp-${func.address}`] ?? "untouched",
        [decompileStatus],
    );

    const filtered = useMemo(() => {
        let list = functions;
        if (category !== "all") list = list.filter((f) => f.category === category);
        if (statusFilter !== "all") list = list.filter((f) => functionStatus(f) === statusFilter);
        if (filter) {
            const lc = filter.toLowerCase();
            list = list.filter(
                (f) =>
                    f.name.toLowerCase().includes(lc) ||
                    f.address.toLowerCase().includes(lc) ||
                    (f.kind ?? "").toLowerCase().includes(lc) ||
                    (f.origin ?? "").toLowerCase().includes(lc) ||
                    (f.source_section ?? "").toLowerCase().includes(lc),
            );
        }
        return list;
    }, [functions, filter, category, statusFilter, functionStatus]);

    const categoryLabel = (cat: CategoryFilter) => {
        switch (cat) {
            case "all": return "All";
            case "import": return "Imp";
            case "export": return "Exp";
            case "internal": return "Code";
            case "thunk": return "Thunk";
            case "external": return "Ext";
            case "debug": return "Dbg";
        }
    };

    const statusLabel = (status: StatusFilter) => {
        switch (status) {
            case "all": return "All";
            case "ready": return "Ready";
            case "error": return "Failed";
            case "loading": return "Running";
            case "untouched": return "New";
            case "idle": return "Idle";
        }
    };

    const statusCount = (status: StatusFilter) => {
        switch (status) {
            case "all": return statusCounts.all;
            case "ready": return statusCounts.ready;
            case "error": return statusCounts.error;
            case "loading": return statusCounts.loading;
            case "untouched": return statusCounts.untouched;
            case "idle": return 0;
        }
    };

    if (functions.length === 0 && !loading) {
        return (
            <div className="functions-list">
                <button className="functions-list__open-btn" onClick={onOpenFile}>
                    <span className="functions-list__open-icon" aria-hidden="true">+</span>
                    <span>Open Binary</span>
                </button>
            </div>
        );
    }

    return (
        <div className="functions-list">
            {functions.length > 0 && (
                <div className="functions-list__summary">
                    <div className="functions-list__summary-item">
                        <span>{summary.internal}</span>
                        <label>Code</label>
                    </div>
                    <div className="functions-list__summary-item">
                        <span>{summary.named}</span>
                        <label>Named</label>
                    </div>
                    <div className="functions-list__summary-item">
                        <span>{summary.imports}</span>
                        <label>Imports</label>
                    </div>
                    <div className="functions-list__summary-item">
                        <span>{summary.exports}</span>
                        <label>Exports</label>
                    </div>
                </div>
            )}

            {/* Category filter */}
            {functions.length > 0 && (
                <div className="functions-list__cats">
                    {(["all", "internal", "export", "import", "thunk", "external", "debug"] as CategoryFilter[]).map((cat) => (
                        <button
                            key={cat}
                            className={`functions-list__cat-btn ${category === cat ? "functions-list__cat-btn--active" : ""}`}
                            onClick={() => setCategory(cat)}
                        >
                            {categoryLabel(cat)}
                            <span className="functions-list__cat-count">{counts[cat]}</span>
                        </button>
                    ))}
                </div>
            )}

            {functions.length > 0 && (
                <div className="functions-list__status-filters">
                    {(["all", "ready", "error", "loading", "untouched"] as StatusFilter[]).map((status) => (
                        <button
                            key={status}
                            className={`functions-list__status-btn ${statusFilter === status ? "functions-list__status-btn--active" : ""}`}
                            onClick={() => setStatusFilter(status)}
                        >
                            <span className={`functions-list__status-dot functions-list__status-dot--${status}`} />
                            {statusLabel(status)}
                            <span className="functions-list__cat-count">{statusCount(status)}</span>
                        </button>
                    ))}
                </div>
            )}

            {/* Search */}
            <div className="functions-list__search">
                <input
                    type="text"
                    placeholder="Filter by name, address, section"
                    value={filter}
                    onChange={(e) => setFilter(e.target.value)}
                    spellCheck={false}
                />
            </div>

            <div className="functions-list__items">
                {loading && <div className="functions-list__loading">Loading functions</div>}
                {!loading && filtered.length === 0 && (
                    <div className="functions-list__empty">No functions match the current filter</div>
                )}
                {filtered.map((f) => (
                    <div
                        key={f.address}
                        className={`functions-list__item ${selectedAddress === f.address ? "functions-list__item--selected" : ""}`}
                        onClick={() => onFunctionClick(f)}
                        onContextMenu={(e) => handleContextMenu(e, f)}
                    >
                        <span
                            className={`functions-list__row-status functions-list__row-status--${functionStatus(f)}`}
                            title={`Decompile: ${statusLabel(functionStatus(f))}`}
                        />
                        <span className="functions-list__item-addr">{f.address}</span>
                        <span className="functions-list__item-main">
                            <span className="functions-list__item-name">{f.name}</span>
                            <span className="functions-list__item-meta">
                                {[f.kind, f.origin, f.source_section].filter(Boolean).join(" · ") || "code"}
                            </span>
                        </span>
                        {f.category !== "internal" && (
                            <span className={`functions-list__item-badge functions-list__item-badge--${f.category}`}>
                                {categoryLabel(f.category).toUpperCase()}
                            </span>
                        )}
                    </div>
                ))}
            </div>

            {/* Right-click context menu */}
            {contextMenu && (
                <div
                    ref={contextMenuRef}
                    className="context-menu"
                    style={{ left: contextMenu.x, top: contextMenu.y }}
                    onClick={(e) => e.stopPropagation()}
                >
                    <div
                        className="context-menu__item"
                        onClick={() => {
                            onFunctionClick(contextMenu.func);
                            closeContextMenu();
                        }}
                    >
                        🔍 Open / Decompile
                    </div>
                    <div
                        className="context-menu__item"
                        onClick={() => {
                            onOpenHex?.(contextMenu.func);
                            closeContextMenu();
                        }}
                    >
                        🔢 Open in Hex Editor
                    </div>
                    <div
                        className="context-menu__item"
                        onClick={() => {
                            onRenameFunc?.(contextMenu.func);
                            closeContextMenu();
                        }}
                    >
                        🏷️ Rename
                    </div>
                    <div
                        className="context-menu__item"
                        onClick={() => {
                            onToggleBookmarkFunc?.(contextMenu.func);
                            closeContextMenu();
                        }}
                    >
                        📌 Add Bookmark
                    </div>
                    <div className="context-menu__separator" />
                    <div
                        className="context-menu__item"
                        onClick={() => {
                            onCopyAddress?.(contextMenu.func.address);
                            navigator.clipboard.writeText(contextMenu.func.address).catch(() => {});
                            closeContextMenu();
                        }}
                    >
                        📋 Copy Address
                    </div>
                </div>
            )}
        </div>
    );
}
