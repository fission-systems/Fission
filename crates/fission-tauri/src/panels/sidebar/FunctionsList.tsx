import { useState, useMemo, useCallback, useEffect, useRef } from "react";
import type { FunctionDto } from "../../types";

type CategoryFilter = "all" | "import" | "export" | "internal";

interface FunctionsListProps {
    functions: FunctionDto[];
    loading: boolean;
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
                        onContextMenu={(e) => handleContextMenu(e, f)}
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

