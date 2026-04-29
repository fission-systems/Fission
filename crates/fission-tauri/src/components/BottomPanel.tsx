import { useState, useCallback, useRef } from "react";
import type { BottomTab, StringDto, ImportDto, BookmarkDto, HexViewData, XrefDto, FunctionDto, PatchRecord, DebugStateDto } from "../types";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { parseAddress } from "../utils/address";
import { MAX_CONSOLE_FUNCS } from "../utils/constants";
import HexView from "../panels/editor/HexView";
import SearchPanel from "../panels/sidebar/SearchPanel";
import XrefsPanel from "../panels/bottom/XrefsPanel";
import CfgPanel from "../panels/bottom/CfgPanel";
import { DebugTab } from "../panels/bottom/DebugTab";
import { StringXrefsPanel } from "../panels/bottom/StringXrefsPanel";
import ExportsPanel from "../panels/bottom/ExportsPanel";
import PatchesPanel from "../panels/bottom/PatchesPanel";
import { NotesPanel } from "../panels/bottom/NotesPanel";
import { TimelinePanel } from "../panels/bottom/TimelinePanel";

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
    /** Single navigation callback — replaces all per-panel address callbacks */
    onNavigate?: (address: string) => void;
    onLog: (msg: string) => void;
    /** Functions list — used by console `funcs` command */
    functions?: FunctionDto[];
    /** Clears the console log array — wired to Edit > Clear Console */
    onClearConsole?: () => void;
    /** List of patches applied to the binary */
    patches?: PatchRecord[];
    /** Revert a patch back to its original bytes */
    onRevertPatch?: (rec: PatchRecord) => void;
    /** Dynamic mode controls Timeline visibility */
    dynamicMode?: boolean;
    /** Debug state for Timeline panel */
    debugState?: DebugStateDto | null;
    /** Undo last action */
    onUndo?: () => void;
    /** Redo last undone action */
    onRedo?: () => void;
    /** Exit the application */
    onExit?: () => void;
    /** Load a binary by path (from console load command) */
    onLoadBinary?: (path: string) => void;
}

const BASE_TABS: { id: BottomTab; label: string }[] = [
    { id: "console", label: "Console" },
    { id: "strings", label: "Strings" },
    { id: "hex", label: "Hex" },
    { id: "imports", label: "Imports" },
    { id: "exports", label: "Exports" },
    { id: "bookmarks", label: "Bookmarks" },
    { id: "xrefs", label: "XRefs" },
    { id: "search", label: "Search" },
    { id: "cfg", label: "CFG" },
    { id: "string-xrefs", label: "StrXRefs" },
    { id: "patches", label: "Patches" },
    { id: "notes", label: "Notes" },
];

const DYNAMIC_TABS: { id: BottomTab; label: string }[] = [
    { id: "debug", label: "Debug" },
    { id: "timeline", label: "Timeline" },
];

// Log-line CSS class based on prefix
function logLineClass(line: string): string {
    if (line.startsWith("[!") || line.toLowerCase().includes("error")) return "console-line--error";
    if (line.startsWith("[✓") || line.startsWith("[OK") || line.startsWith("[*")) return "console-line--success";
    if (line.startsWith(">")) return "console-line--cmd";
    return "";
}

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
    onNavigate,
    onLog,
    functions,
    onClearConsole,
    patches = [],
    onRevertPatch,
    dynamicMode = false,
    debugState = null,
    onUndo,
    onRedo,
    onExit,
    onLoadBinary,
}: BottomPanelProps) {
    const [cmdInput, setCmdInput] = useState("");
    const [cmdHistory, setCmdHistory] = useState<string[]>([]);
    const cmdHistoryIdx = useRef(-1);
    const consoleEndRef = useRef<HTMLDivElement>(null);

    const handleConsoleCommand = useCallback((raw: string) => {
        const cmd = raw.trim();
        if (!cmd) return;
        setCmdHistory((prev) => [cmd, ...prev]);
        cmdHistoryIdx.current = -1;
        onLog(`> ${cmd}`);

        const [verb, ...args] = cmd.split(/\s+/);
        switch (verb.toLowerCase()) {
            case "help":
                onLog("Commands:");
                onLog("  help                         — show this help");
                onLog("  funcs                        — list loaded functions");
                onLog("  clear                        — clear console");
                onLog("  goto <addr>                  — navigate to address");
                onLog("  rename <addr> <name>         — rename function");
                onLog("  comment <addr> <text>        — set comment");
                onLog("  load <path>                  — load binary from path");
                onLog("  patch <addr> <byte> [bytes]  — patch bytes (hex)");
                onLog("  plugin load <path>           — load a Rust plugin");
                onLog("  plugin list                  — list loaded plugins");
                onLog("  undo                         — undo last action");
                onLog("  redo                         — redo last undone action");
                onLog("  exit                         — quit Fission");
                break;
            case "funcs":
                if (!functions || functions.length === 0) {
                    onLog("No functions loaded.");
                } else {
                    onLog(`${functions.length} functions:`);
                    functions.slice(0, MAX_CONSOLE_FUNCS).forEach((f) => onLog(`  ${f.address}  ${f.name}`));
                    if (functions.length > MAX_CONSOLE_FUNCS) onLog(`  ... (${functions.length - MAX_CONSOLE_FUNCS} more)`);
                }
                break;
            case "clear":
                if (onClearConsole) {
                    onClearConsole();
                } else {
                    onLog("[Console cleared]");
                }
                break;
            case "goto":
                if (!args[0]) { onLog("Usage: goto <address>"); break; }
                onNavigate?.(args[0]);
                break;
            case "rename":
                if (!args[0] || !args[1]) { onLog("Usage: rename <address> <new_name>"); break; }
                (async () => {
                    try {
                        const addr = parseAddress(args[0]);
                        await invoke("rename_function", { address: addr, newName: args.slice(1).join(" ") });
                        onLog(`[✓] Renamed ${args[0]} → ${args.slice(1).join(" ")}`);
                    } catch (e) { onLog(`[!] Error: ${e}`); }
                })();
                break;
            case "comment":
                if (!args[0]) { onLog("Usage: comment <address> <text>"); break; }
                (async () => {
                    try {
                        const addr = parseAddress(args[0]);
                        await invoke("add_comment", { address: addr, text: args.slice(1).join(" ") });
                        onLog(`[✓] Comment set at ${args[0]}`);
                    } catch (e) { onLog(`[!] Error: ${e}`); }
                })();
                break;
            case "load":
                if (!args[0]) { onLog("Usage: load <path>"); break; }
                if (onLoadBinary) {
                    onLoadBinary(args.join(" "));
                } else {
                    onLog("[!] load handler not connected");
                }
                break;
            case "patch":
                if (!args[0] || !args[1]) { onLog("Usage: patch <address> <hex_byte> [hex_byte...]"); break; }
                (async () => {
                    try {
                        const addr = parseAddress(args[0]);
                        const bytes = args.slice(1).map((b) => parseInt(b, 16));
                        if (bytes.some(isNaN)) { onLog("[!] Invalid hex byte(s)"); return; }
                        await invoke("patch_bytes", { address: addr, bytes });
                        onLog(`[✓] Patched ${bytes.length} byte(s) at ${args[0]}`);
                    } catch (e) { onLog(`[!] Error: ${e}`); }
                })();
                break;
            case "plugin":
                if (args[0] === "load") {
                    const path = args.slice(1).join(" ");
                    if (!path) { onLog("Usage: plugin load <path>"); break; }
                    (async () => {
                        try {
                            const info = await invoke<{ name: string; version: string }>("load_plugin", { path });
                            onLog(`[✓] Plugin loaded: ${info.name} v${info.version}`);
                        } catch (e) { onLog(`[!] Plugin load failed: ${e}`); }
                    })();
                } else if (args[0] === "list") {
                    (async () => {
                        try {
                            const plugins = await invoke<{ id: string; name: string; version: string; enabled: boolean }[]>("list_plugins");
                            if (plugins.length === 0) { onLog("No plugins loaded."); }
                            else {
                                onLog(`${plugins.length} plugin(s):`);
                                plugins.forEach((p) => onLog(`  ${p.enabled ? "[✓]" : "[ ]"} ${p.name} (${p.id}) v${p.version}`));
                            }
                        } catch (e) { onLog(`[!] ${e}`); }
                    })();
                } else {
                    onLog("Usage: plugin load <path> | plugin list");
                }
                break;
            case "undo":
                if (onUndo) onUndo();
                else onLog("[!] Undo not available");
                break;
            case "redo":
                if (onRedo) onRedo();
                else onLog("[!] Redo not available");
                break;
            case "exit":
            case "quit":
                if (onExit) onExit();
                else getCurrentWindow().close();
                break;
            default:
                onLog(`Unknown command: ${verb}. Type 'help' for a list.`);
        }
        setCmdInput("");
    }, [functions, onNavigate, onLog, onClearConsole, onUndo, onRedo, onExit, onLoadBinary]);

    const handleKeyDown = useCallback((e: React.KeyboardEvent<HTMLInputElement>) => {
        if (e.key === "Enter") {
            handleConsoleCommand(cmdInput);
        } else if (e.key === "ArrowUp") {
            e.preventDefault();
            const nextIdx = Math.min(cmdHistoryIdx.current + 1, cmdHistory.length - 1);
            cmdHistoryIdx.current = nextIdx;
            if (nextIdx >= 0) setCmdInput(cmdHistory[nextIdx]);
        } else if (e.key === "ArrowDown") {
            e.preventDefault();
            const nextIdx = Math.max(cmdHistoryIdx.current - 1, -1);
            cmdHistoryIdx.current = nextIdx;
            setCmdInput(nextIdx >= 0 ? cmdHistory[nextIdx] : "");
        }
    }, [cmdInput, cmdHistory, handleConsoleCommand]);

    return (
        <div className="bottom-panel" style={{ height }}>
            <div className="bottom-panel__tabs">
                {[...BASE_TABS, ...(dynamicMode ? DYNAMIC_TABS : [])].map((tab) => (
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
                    <div className="console-output-wrap">
                        <div className="console-output">
                            {logs.map((line, i) => (
                                <div key={i} className={`console-line ${logLineClass(line)}`}>{line}</div>
                            ))}
                            <div ref={consoleEndRef} />
                        </div>
                        <div className="console-toolbar">
                            <button
                                className="console-copy-btn"
                                title="Copy all console output"
                                onClick={() => navigator.clipboard.writeText(logs.join("\n"))}
                            >
                                ⧅ Copy All
                            </button>
                        </div>
                        <div className="console-cli">
                            <span className="console-cli__prompt">&gt;</span>
                            <input
                                className="console-cli__input"
                                type="text"
                                value={cmdInput}
                                onChange={(e) => setCmdInput(e.target.value)}
                                onKeyDown={handleKeyDown}
                                placeholder="Type a command (help for list)"
                                spellCheck={false}
                                autoComplete="off"
                            />
                        </div>
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
                                        onClick={() => onNavigate?.(s.offset)}
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
                    <HexView data={hexData} onAddressClick={onNavigate} />
                )}

                {activeTab === "imports" && (
                    <div className="imports-table-wrap">
                        <table className="data-table">
                            <thead>
                                <tr>
                                    <th>Address</th>
                                    <th>Library</th>
                                    <th>Function</th>
                                    <th>Kind</th>
                                    <th>Origin</th>
                                    <th>Section</th>
                                </tr>
                            </thead>
                            <tbody>
                                {imports.map((imp, i) => (
                                    <tr
                                        key={i}
                                        className="data-table__row"
                                        onClick={() => onNavigate?.(imp.address)}
                                    >
                                        <td className="data-table__addr">{imp.address}</td>
                                        <td className="data-table__lib">{imp.library}</td>
                                        <td className="data-table__name">{imp.name}</td>
                                        <td className="data-table__enc">{imp.kind ?? "import"}</td>
                                        <td className="data-table__enc">{imp.origin ?? "—"}</td>
                                        <td className="data-table__enc">{imp.source_section ?? "—"}</td>
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
                                            onClick={() => onNavigate?.(bm.address)}
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
                    <XrefsPanel xrefs={xrefs} address={xrefAddress} onXrefClick={onNavigate} />
                )}

                {activeTab === "search" && (
                    <SearchPanel binaryLoaded={binaryLoaded} onResultClick={onNavigate} />
                )}

                {activeTab === "cfg" && (
                    <CfgPanel
                        address={cfgAddress}
                        binaryLoaded={binaryLoaded}
                        onLog={onLog}
                    />
                )}

                {activeTab === "string-xrefs" && (
                    <StringXrefsPanel
                        binaryLoaded={binaryLoaded}
                        onLog={onLog}
                        onAddressClick={onNavigate}
                    />
                )}

                {activeTab === "exports" && (
                    <ExportsPanel
                        binaryLoaded={binaryLoaded}
                        onExportClick={onNavigate}
                    />
                )}

                {activeTab === "patches" && (
                    <PatchesPanel
                        patches={patches}
                        onRevert={onRevertPatch}
                    />
                )}

                {activeTab === "notes" && (
                    <NotesPanel
                        binaryLoaded={binaryLoaded}
                        onNoteClick={onNavigate}
                    />
                )}

                {(activeTab === "debug" || activeTab === "timeline") && !dynamicMode && (
                    <div className="timeline-panel timeline-panel--static">
                        <p>Debug / Timeline requires Dynamic mode.</p>
                        <p className="timeline-panel__hint">
                            Enable Dynamic mode from the Debug menu or Status Bar.
                        </p>
                    </div>
                )}

                {activeTab === "debug" && dynamicMode && (
                    <DebugTab onLog={onLog} />
                )}

                {activeTab === "timeline" && (
                    <TimelinePanel debugState={debugState} dynamicMode={dynamicMode} />
                )}
            </div>
        </div>
    );
}
