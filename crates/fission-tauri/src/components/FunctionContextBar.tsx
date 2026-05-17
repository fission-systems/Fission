import type { EditorTab, FunctionDto } from "../types";

type DecompileStatus = "idle" | "loading" | "ready" | "error";

interface FunctionContextBarProps {
    tab: EditorTab | null;
    func: FunctionDto | null;
    status: DecompileStatus;
    onAssembly: () => void;
    onDecompile: () => void;
    onRename: () => void;
    onComment: () => void;
    onXrefs: () => void;
}

function statusLabel(status: DecompileStatus): string {
    switch (status) {
        case "loading":
            return "Decompiling";
        case "ready":
            return "Ready";
        case "error":
            return "Failed";
        case "idle":
            return "Idle";
    }
}

export default function FunctionContextBar({
    tab,
    func,
    status,
    onAssembly,
    onDecompile,
    onRename,
    onComment,
    onXrefs,
}: FunctionContextBarProps) {
    if (!tab || tab.type === "listing") {
        return null;
    }

    const section = func?.source_section ?? "section ?";
    const kind = func?.kind ?? func?.origin ?? "function";
    const size = func?.size ? `${func.size} bytes` : "size ?";

    return (
        <div className="function-context">
            <div className="function-context__identity">
                <span className={`function-context__status function-context__status--${status}`} />
                <div className="function-context__title-group">
                    <div className="function-context__title">{tab.functionName}</div>
                    <div className="function-context__meta">
                        <span>{tab.address}</span>
                        <span>{section}</span>
                        <span>{kind}</span>
                        <span>{size}</span>
                        <span>{statusLabel(status)}</span>
                    </div>
                </div>
            </div>
            <div className="function-context__actions">
                <button onClick={onDecompile} disabled={tab.type === "decompile"} title="Open decompile view">
                    Decompile
                </button>
                <button onClick={onAssembly} disabled={tab.type === "assembly"} title="Open assembly view">
                    ASM
                </button>
                <button onClick={onXrefs} title="Show cross references">
                    XRefs
                </button>
                <button onClick={onRename} title="Rename symbol">
                    Rename
                </button>
                <button onClick={onComment} title="Add or edit comment">
                    Comment
                </button>
            </div>
        </div>
    );
}
