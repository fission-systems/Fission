import type { BottomTab, StringDto } from "../types";

interface Props {
    activeTab: BottomTab;
    onTabChange: (tab: BottomTab) => void;
    height: number;
    logs: string[];
    strings: StringDto[];
}

export default function BottomPanel({ activeTab, onTabChange, height, logs, strings }: Props) {
    return (
        <div className="bottom-panel" style={{ height }}>
            <div className="bottom-panel__tabs">
                <div
                    className={`bottom-panel__tab ${activeTab === "console" ? "bottom-panel__tab--active" : ""}`}
                    onClick={() => onTabChange("console")}
                >
                    Console
                </div>
                <div
                    className={`bottom-panel__tab ${activeTab === "strings" ? "bottom-panel__tab--active" : ""}`}
                    onClick={() => onTabChange("strings")}
                >
                    Strings ({strings.length})
                </div>
            </div>

            <div className="bottom-panel__content">
                {activeTab === "console" ? (
                    <div>
                        {logs.map((line, i) => (
                            <div key={i}>{line}</div>
                        ))}
                    </div>
                ) : (
                    <div className="string-table">
                        {strings.length === 0 ? (
                            <div className="empty-state">No strings extracted</div>
                        ) : (
                            strings.map((s, i) => (
                                <div key={i} className="string-table__row">
                                    <span className="string-table__offset">{s.offset}</span>
                                    <span className="string-table__value">{s.value}</span>
                                    <span className="string-table__encoding">{s.encoding}</span>
                                </div>
                            ))
                        )}
                    </div>
                )}
            </div>
        </div>
    );
}
