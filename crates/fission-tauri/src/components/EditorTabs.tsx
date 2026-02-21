import type { EditorTab } from "../types";

interface EditorTabsProps {
    tabs: EditorTab[];
    activeTabId: string | null;
    onTabClick: (id: string) => void;
    onTabClose: (id: string) => void;
    canGoBack: boolean;
    canGoForward: boolean;
    onGoBack: () => void;
    onGoForward: () => void;
}

function tabIcon(type: EditorTab["type"]): string {
    switch (type) {
        case "decompile": return "{ }";
        case "assembly": return "≡";
        case "listing": return "☰";
        case "hexview": return "⬡";
    }
}

export default function EditorTabs({
    tabs,
    activeTabId,
    onTabClick,
    onTabClose,
    canGoBack,
    canGoForward,
    onGoBack,
    onGoForward,
}: EditorTabsProps) {
    return (
        <div className="editor-tabs">
            <div className="editor-tabs__nav">
                <button
                    className="editor-tabs__nav-btn"
                    disabled={!canGoBack}
                    onClick={onGoBack}
                    title="Go Back (Alt+←)"
                >
                    ◀
                </button>
                <button
                    className="editor-tabs__nav-btn"
                    disabled={!canGoForward}
                    onClick={onGoForward}
                    title="Go Forward (Alt+→)"
                >
                    ▶
                </button>
            </div>
            <div className="editor-tabs__list">
                {tabs.map((tab) => (
                    <div
                        key={tab.id}
                        className={`editor-tab ${activeTabId === tab.id ? "editor-tab--active" : ""}`}
                        onClick={() => onTabClick(tab.id)}
                    >
                        <span className="editor-tab__icon">{tabIcon(tab.type)}</span>
                        <span className="editor-tab__title">{tab.title}</span>
                        <button
                            className="editor-tab__close"
                            onClick={(e) => {
                                e.stopPropagation();
                                onTabClose(tab.id);
                            }}
                        >
                            ×
                        </button>
                    </div>
                ))}
            </div>
        </div>
    );
}
