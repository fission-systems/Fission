import type { BottomTab, StringDto, ImportDto, BookmarkDto, HexViewData, XrefDto } from "../types";
import HexView from "../panels/HexView";
import SearchPanel from "../panels/SearchPanel";
import XrefsPanel from "../panels/XrefsPanel";
import CfgPanel from "../panels/CfgPanel";
import { DebugTab } from "../panels/DebugTab";
import { StringXrefsPanel } from "../panels/StringXrefsPanel";

interface BottomPanelProps {
    activeTab: BottomTab;
    onTabChange: (tab: BottomTab) => void;
    height: number;
    logs: string[];
    strings: StringDto[];
    imports: ImportDto[];
    bookmarks: BookmarkDto[];
    hexData: HexViewData | null;
    xrefs: XrefDto[];
    xrefAddress: string | null;
    /** Address of the currently selected function — used by CFG panel */
    cfgAddress: string | null;
    binaryLoaded: boolean;
    onBookmarkClick?: (address: string) => void;
    onImportClick?: (address: string) => void;
    onStringClick?: (offset: string) => void;
    onSearchResultClick?: (address: string) => void;
    onXrefClick?: (address: string) => void;
    onLog: (msg: string) => void;
}

const TABS: { id: BottomTab; label: string }[] = [
    { id: "console", label: "Console" },
    { id: "strings", label: "Strings" },
    { id: "hex", label: "Hex" },
    { id: "imports", label: "Imports" },
    { id: "bookmarks", label: "Bookmarks" },
    { id: "xrefs", label: "XRefs" },
    { id: "search", label: "Search" },
    { id: "cfg", label: "CFG" },
    { id: "debug", label: "Debug" },
    { id: "string-xrefs", label: "StrXRefs" },
];

export default function BottomPanel({
    activeTab,
    onTabChange,
    height,
    logs,
    strings,
    imports,
    bookmarks,
    hexData,
    xrefs,
    xrefAddress,
    cfgAddress,
    binaryLoaded,
    onBookmarkClick,
    onImportClick,
    onStringClick,
    onSearchResultClick,
    onXrefClick,
    onLog,
}: BottomPanelProps) {
    return (
        <div className="bottom-panel" style={{ height }}>
            <div className="bottom-panel__tabs">
                {TABS.map((tab) => (
                    <button
                        key={tab.id}
                        className={`bottom-panel__tab ${activeTab === tab.id ? "bottom-panel__tab--active" : ""}`}
                        onClick={() => onTabChange(tab.id)}
                    >
                        {tab.label}
                        {tab.id === "bookmarks" && bookmarks.length > 0 && (
                            <span className="bottom-panel__badge">{bookmarks.length}</span>
                        )}
                    </button>
                ))}
            </div>

            <div className="bottom-panel__content">
                {activeTab === "console" && (
                    <div className="console-output">
                        {logs.map((line, i) => (
                            <div key={i} className="console-line">{line}</div>
                        ))}
                    </div>
                )}

                {activeTab === "strings" && (
                    <div className="strings-table-wrap">
                        <table className="data-table">
                            <thead>
                                <tr>
                                    <th>Offset</th>
                                    <th>Encoding</th>
                                    <th>Value</th>
                                </tr>
                            </thead>
                            <tbody>
                                {strings.map((s, i) => (
                                    <tr
                                        key={i}
                                        className="data-table__row"
                                        onClick={() => onStringClick?.(s.offset)}
                                    >
                                        <td className="data-table__addr">{s.offset}</td>
                                        <td className="data-table__enc">{s.encoding}</td>
                                        <td className="data-table__val">{s.value}</td>
                                    </tr>
                                ))}
                            </tbody>
                        </table>
                    </div>
                )}

                {activeTab === "hex" && (
                    <HexView data={hexData} />
                )}

                {activeTab === "imports" && (
                    <div className="imports-table-wrap">
                        <table className="data-table">
                            <thead>
                                <tr>
                                    <th>Address</th>
                                    <th>Library</th>
                                    <th>Function</th>
                                </tr>
                            </thead>
                            <tbody>
                                {imports.map((imp, i) => (
                                    <tr
                                        key={i}
                                        className="data-table__row"
                                        onClick={() => onImportClick?.(imp.address)}
                                    >
                                        <td className="data-table__addr">{imp.address}</td>
                                        <td className="data-table__lib">{imp.library}</td>
                                        <td className="data-table__name">{imp.name}</td>
                                    </tr>
                                ))}
                            </tbody>
                        </table>
                    </div>
                )}

                {activeTab === "bookmarks" && (
                    <div className="bookmarks-panel">
                        {bookmarks.length === 0 ? (
                            <div className="bookmarks-empty">
                                No bookmarks yet. Press <kbd>F2</kbd> to add one.
                            </div>
                        ) : (
                            <table className="data-table">
                                <thead>
                                    <tr>
                                        <th>Address</th>
                                        <th>Label</th>
                                        <th>Function</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {bookmarks.map((bm, i) => (
                                        <tr
                                            key={i}
                                            className="data-table__row"
                                            onClick={() => onBookmarkClick?.(bm.address)}
                                        >
                                            <td className="data-table__addr">{bm.address}</td>
                                            <td>{bm.label}</td>
                                            <td>{bm.function_name || "—"}</td>
                                        </tr>
                                    ))}
                                </tbody>
                            </table>
                        )}
                    </div>
                )}

                {activeTab === "xrefs" && (
                    <XrefsPanel xrefs={xrefs} address={xrefAddress} onXrefClick={onXrefClick} />
                )}

                {activeTab === "search" && (
                    <SearchPanel binaryLoaded={binaryLoaded} onResultClick={onSearchResultClick} />
                )}

                {activeTab === "cfg" && (
                    <CfgPanel
                        address={cfgAddress}
                        binaryLoaded={binaryLoaded}
                        onLog={onLog}
                    />
                )}

                {activeTab === "debug" && (
                    <DebugTab onLog={onLog} />
                )}

                {activeTab === "string-xrefs" && (
                    <StringXrefsPanel
                        binaryLoaded={binaryLoaded}
                        onLog={onLog}
                    />
                )}
            </div>
        </div>
    );
}
