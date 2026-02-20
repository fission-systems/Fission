import type { EditorTab } from "../types";

interface Props {
    tabs: EditorTab[];
    activeTabId: string | null;
    onTabClick: (id: string) => void;
    onTabClose: (id: string) => void;
}

export default function EditorTabs({ tabs, activeTabId, onTabClick, onTabClose }: Props) {
    if (tabs.length === 0) return <div className="editor-tabs" />;

    return (
        <div className="editor-tabs">
            {tabs.map((tab) => (
                <div
                    key={tab.id}
                    className={`editor-tab ${tab.id === activeTabId ? "editor-tab--active" : ""}`}
                    onClick={() => onTabClick(tab.id)}
                >
                    <span className="editor-tab__icon">
                        {tab.type === "decompile" ? "{ }" : "⚙"}
                    </span>
                    <span>{tab.title}</span>
                    <span
                        className="editor-tab__close"
                        onClick={(e) => {
                            e.stopPropagation();
                            onTabClose(tab.id);
                        }}
                    >
                        ×
                    </span>
                </div>
            ))}
        </div>
    );
}
