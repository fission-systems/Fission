import { useState, useCallback, useRef, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open, save } from "@tauri-apps/plugin-dialog";
import type {
    FunctionDto,
    ActivityView,
    BottomTab,
    BookmarkDto,
    GotoResult,
    AppSettings,
    UndoableAction,
    FissionProject,
} from "./types";
import { ListingView } from "./panels/editor/ListingView";
import { DebugSidebar } from "./panels/sidebar/DebugSidebar";
import ActivityBar from "./components/ActivityBar";
import Sidebar from "./components/Sidebar";
import EditorTabs from "./components/EditorTabs";
import StatusBar from "./components/StatusBar";
import BottomPanel from "./components/BottomPanel";
import GotoDialog from "./panels/dialogs/GotoDialog";
import RenameDialog from "./panels/dialogs/RenameDialog";
import CommentDialog from "./panels/dialogs/CommentDialog";
import FunctionsList from "./panels/sidebar/FunctionsList";
import DecompileView from "./panels/editor/DecompileView";
import AssemblyView from "./panels/editor/AssemblyView";
import HexView from "./panels/editor/HexView";
import SettingsPanel from "./panels/sidebar/SettingsPanel";
import SectionsPanel from "./panels/sidebar/SectionsPanel";
import AboutDialog from "./panels/dialogs/AboutDialog";
import DecompilerOptionsDialog from "./panels/dialogs/DecompilerOptionsDialog";
import SearchPanel from "./panels/sidebar/SearchPanel";
import PluginsPanel from "./panels/sidebar/PluginsPanel";
import { useEditorTabs } from "./hooks/useEditorTabs";
import { useBinary } from "./hooks/useBinary";
import { useDebug } from "./hooks/useDebug";
import { useDialogs } from "./hooks/useDialogs";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";
import { useMenuEvents } from "./hooks/useMenuEvents";
import { useDragAndDrop } from "./hooks/useDragAndDrop";
import { parseAddress } from "./utils/address";

function App() {
    // --- Hooks ---
    const {
        tabs, activeTabId,
        canGoBack, canGoForward, goBack, goForward,
        handleTabClick, handleCloseTab,
        openFunctionTabs, openListingTab, openHexTab,
        openAssemblyTab, openDecompileTabFromActive,
        resetTabs, updateTabNames,
    } = useEditorTabs();

    // log is defined before useBinary because useBinary needs it as a callback
    const [logs, setLogs] = useState<string[]>(["[Fission] Ready."]);
    const log = useCallback((msg: string) => {
        setLogs((prev) => [...prev, `[${new Date().toLocaleTimeString()}] ${msg}`]);
    }, []);

    const bin = useBinary({
        log,
        onOpenTabs: openFunctionTabs,
        onResetTabs: resetTabs,
    });
    const {
        binaryInfo,
        functions, setFunctions,
        sections, strings, imports, bookmarks, setBookmarks,
        patches, hexData, xrefs,
        loading, progress, fidRunning,
        decompileCache, asmCache, asmHasMore, asmLoadingMore,
        handleOpenFile, handleLoadBinary, handleRunFid,
        handleFunctionClick, handleAsmLoadMore,
        handleToggleBookmark: handleToggleBookmarkAt,
        handleRecordPatch, handleRevertPatch,
        handleClearCache: handleClearDecompileCache,
        clearAsmCache,
    } = bin;

    const { dynamicMode, gitBranch, handleToggleDynamicMode } = useDebug({ log });

    // --- UI State ---
    const [activeView, setActiveView] = useState<ActivityView>("explorer");
    const [bottomTab, setBottomTab] = useState<BottomTab>("console");
    const [bottomPanelHeight, setBottomPanelHeight] = useState(200);
    const [bottomPanelVisible, setBottomPanelVisible] = useState(true);
    const [sidebarVisible, setSidebarVisible] = useState(true);

    // Settings
    const [settings, setSettings] = useState<AppSettings>({
        theme: "dark",
        font_size: 14,
        decompile_style: "c-like",
        simplify_level: 1,
    });

    // Dialog state
    const {
        gotoOpen, setGotoOpen,
        renameOpen, setRenameOpen, renameTarget, openRename,
        commentOpen, setCommentOpen, commentTarget, openComment,
        aboutOpen, setAboutOpen,
        decompilerOptionsOpen, setDecompilerOptionsOpen,
    } = useDialogs();

    // Undo/Redo stacks
    const undoStack = useRef<UndoableAction[]>([]);
    const redoStack = useRef<UndoableAction[]>([]);

    // --- Project Save ---
    const handleSaveProject = useCallback(async () => {
        if (!binaryInfo) return;
        try {
            const path = await save({
                filters: [{ name: "Fission Project", extensions: ["fprj"] }],
                defaultPath: `${binaryInfo.name}.fprj`,
            });
            if (!path) return;
            await invoke("save_project", { path });
            log(`Project saved: ${path}`);
        } catch (err) {
            log(`Save project error: ${err}`);
        }
    }, [binaryInfo, log]);

    // --- Phase 8: Export Analysis JSON ---
    const handleExportJson = useCallback(async () => {
        if (!binaryInfo) return;
        try {
            const path = await save({
                filters: [{ name: "JSON", extensions: ["json"] }],
                defaultPath: `${binaryInfo.name}_analysis.json`,
            });
            if (!path) return;
            await invoke("export_analysis_json", { path });
            log(`Analysis exported: ${path}`);
        } catch (err) {
            log(`Export JSON error: ${err}`);
        }
    }, [binaryInfo, log]);

    // --- Project Load ---
    const handleLoadProject = useCallback(async () => {
        try {
            const selected = await open({
                multiple: false,
                filters: [{ name: "Fission Project", extensions: ["fprj"] }],
            });
            if (!selected) return;
            const path = selected == null ? null : Array.isArray(selected) ? selected[0] : selected;
            if (!path) return;
            const project = await invoke<FissionProject>("load_project", { path });
            log(`Project loaded from: ${path}`);

            if (!binaryInfo || binaryInfo.path !== project.binary_path) {
                log(`Reloading binary: ${project.binary_path}`);
                await handleLoadBinary(project.binary_path);
            }

            invoke<BookmarkDto[]>("get_bookmarks").then(setBookmarks);
            invoke<FunctionDto[]>("get_functions").then(setFunctions);
            log("Annotations restored (comments, renames, bookmarks).");
        } catch (err) {
            log(`Load project error: ${err}`);
        }
    }, [binaryInfo, log, handleLoadBinary, setBookmarks, setFunctions]);

    // --- Clear Console ---
    const handleClearConsole = useCallback(() => {
        setLogs([]);
    }, []);

    // --- Exit ---
    const handleExit = useCallback(() => {
        getCurrentWindow().close();
    }, []);

    // --- Toggle Developer Tools ---
    // Now handled natively by Rust on_menu_event — keep for keyboard shortcut fallback
    const handleToggleDevTools = useCallback(() => {
        invoke("toggle_devtools").catch((e) => log(`[DevTools error] ${e}`));
    }, []);

    // --- Toggle Bottom Panel ---
    const handleToggleBottomPanel = useCallback(() => {
        setBottomPanelVisible((v) => !v);
    }, []);

    // --- Open Listing Tab ---
    const handleOpenListingTab = useCallback(() => {
        openListingTab(binaryInfo);
    }, [binaryInfo, openListingTab]);

    // --- Open Hex Editor Tab ---
    const handleOpenHexTab = useCallback((func: FunctionDto) => {
        openHexTab(func, binaryInfo);
    }, [binaryInfo, openHexTab]);

    // --- Load settings on startup ---
    useEffect(() => {
        invoke<AppSettings>("get_settings")
            .then(setSettings)
            .catch((e) => console.warn("get_settings failed:", e));
    }, []);

    // --- File Open, FunctionClick, AsmLoadMore, FID → moved to useBinary hook ---

    // --- Navigate to address (from assembly click, goto, bookmark) ---
    const handleNavigateToAddress = useCallback(
        async (addressStr: string) => {
            try {
                const result = await invoke<GotoResult>("goto_address", { input: addressStr });
                if (result.found && result.function_name) {
                    // Find or create the function entry
                    const func = functions.find(
                        (f) => f.address.toLowerCase() === result.address.toLowerCase(),
                    );
                    if (func) {
                        handleFunctionClick(func);
                    } else {
                        // Create a synthetic entry
                        handleFunctionClick({
                            address: result.address,
                            name: result.function_name || `sub_${result.address.slice(2)}`,
                            size: 0,
                            category: "internal",
                        });
                    }
                    log(`Navigated to ${result.address} (${result.function_name})`);
                } else {
                    log(`Address not found: ${addressStr}`);
                }
            } catch (err) {
                log(`Goto error: ${err}`);
            }
        },
        [functions, handleFunctionClick, log],
    );

    // --- Rename (with undo tracking) ---
    const handleRename = useCallback(
        async (address: string, newName: string) => {
            try {
                const addr = parseAddress(address);
                const prevFunc = functions.find((f) => f.address.toLowerCase() === address.toLowerCase());
                const prevName = prevFunc?.name ?? "";
                await invoke("rename_function", { address: addr, newName });
                log(`Renamed: ${address} → ${newName || "(reverted)"}`);
                undoStack.current.push({ type: "rename", address, previousValue: prevName, newValue: newName });
                redoStack.current = [];
                const funcs = await invoke<FunctionDto[]>("get_functions");
                setFunctions(funcs);
                if (newName) updateTabNames(address, newName);
            } catch (err) {
                log(`Rename error: ${err}`);
            }
        },
        [log, functions, setFunctions, updateTabNames],
    );

    // --- Comment (with undo tracking) ---
    const handleAddComment = useCallback(
        async (address: string, text: string) => {
            try {
                const addr = parseAddress(address);
                await invoke("add_comment", { address: addr, text });
                log(`Comment at ${address}: ${text || "(removed)"}`);
                undoStack.current.push({ type: "comment", address, previousValue: "", newValue: text });
                redoStack.current = [];
                clearAsmCache();
            } catch (err) {
                log(`Comment error: ${err}`);
            }
        },
        [log, clearAsmCache],
    );

    // --- Save Snapshot ---
    const handleSaveSnapshot = useCallback(async () => {
        try {
            const path = await save({
                filters: [{ name: "Fission Snapshot", extensions: ["fsnap"] }],
                defaultPath: "snapshot.fsnap",
            });
            if (!path) return;
            await invoke("save_snapshot", { path });
            log(`Snapshot saved: ${path}`);
        } catch (err) {
            log(`Save snapshot error: ${err}`);
        }
    }, [log]);

    // --- Load Snapshot ---
    const handleLoadSnapshot = useCallback(async () => {
        try {
            const selected = await open({
                multiple: false,
                filters: [{ name: "Fission Snapshot", extensions: ["fsnap"] }],
            });
            if (!selected) return;
            const path = Array.isArray(selected) ? selected[0] : selected;
            await invoke("load_snapshot", { path });
            invoke<BookmarkDto[]>("get_bookmarks").then(setBookmarks);
            invoke<FunctionDto[]>("get_functions").then(setFunctions);
            log(`Snapshot loaded from: ${path}`);
        } catch (err) {
            log(`Load snapshot error: ${err}`);
        }
    }, [log]);

    // --- Toggle Sidebar ---
    const handleToggleSidebar = useCallback(() => {
        setSidebarVisible((v) => !v);
    }, []);

    // --- Open Assembly / Decompile view from menu ---
    const handleOpenAssemblyView = useCallback(() => {
        const tab = tabs.find((t) => t.id === activeTabId) ?? null;
        openAssemblyTab(tab, binaryInfo);
    }, [tabs, activeTabId, binaryInfo, openAssemblyTab]);

    const handleOpenDecompileView = useCallback(() => {
        const tab = tabs.find((t) => t.id === activeTabId) ?? null;
        openDecompileTabFromActive(tab, binaryInfo);
    }, [tabs, activeTabId, binaryInfo, openDecompileTabFromActive]);

    // --- Undo / Redo ---
    const handleUndo = useCallback(async () => {
        const action = undoStack.current.pop();
        if (!action) { log("Nothing to undo."); return; }
        try {
            if (action.type === "rename") {
                const addr = parseAddress(action.address);
                await invoke("rename_function", { address: addr, newName: action.previousValue });
                const funcs = await invoke<FunctionDto[]>("get_functions");
                setFunctions(funcs);
                updateTabNames(action.address, action.previousValue);
                log(`Undo rename: ${action.address} → ${action.previousValue}`);
            } else if (action.type === "comment") {
                const addr = parseAddress(action.address);
                await invoke("add_comment", { address: addr, text: action.previousValue });
                clearAsmCache();
                log(`Undo comment at ${action.address}`);
            }
            redoStack.current.push(action);
        } catch (err) {
            log(`Undo error: ${err}`);
        }
    }, [log, setFunctions, updateTabNames, clearAsmCache]);

    const handleRedo = useCallback(async () => {
        const action = redoStack.current.pop();
        if (!action) { log("Nothing to redo."); return; }
        try {
            if (action.type === "rename") {
                const addr = parseAddress(action.address);
                await invoke("rename_function", { address: addr, newName: action.newValue });
                const funcs = await invoke<FunctionDto[]>("get_functions");
                setFunctions(funcs);
                updateTabNames(action.address, action.newValue);
                log(`Redo rename: ${action.address} → ${action.newValue}`);
            } else if (action.type === "comment") {
                const addr = parseAddress(action.address);
                await invoke("add_comment", { address: addr, text: action.newValue });
                clearAsmCache();
                log(`Redo comment at ${action.address}`);
            }
            undoStack.current.push(action);
        } catch (err) {
            log(`Redo error: ${err}`);
        }
    }, [log, setFunctions, updateTabNames, clearAsmCache]);

    // --- Bookmark (active tab) ---
    const handleToggleBookmark = useCallback(() => {
        const tab = tabs.find((t) => t.id === activeTabId);
        if (!tab) return;
        handleToggleBookmarkAt(tab.address, tab.functionName);
    }, [tabs, activeTabId, handleToggleBookmarkAt]);

    // --- Resize bottom panel ---
    const resizeRef = useRef<{ startY: number; startH: number } | null>(null);

    const handleResizeStart = useCallback(
        (e: React.MouseEvent) => {
            e.preventDefault();
            resizeRef.current = { startY: e.clientY, startH: bottomPanelHeight };
            const handleMouseMove = (e: MouseEvent) => {
                if (!resizeRef.current) return;
                const delta = resizeRef.current.startY - e.clientY;
                setBottomPanelHeight(Math.max(100, Math.min(600, resizeRef.current.startH + delta)));
            };
            const handleMouseUp = () => {
                resizeRef.current = null;
                document.removeEventListener("mousemove", handleMouseMove);
                document.removeEventListener("mouseup", handleMouseUp);
            };
            document.addEventListener("mousemove", handleMouseMove);
            document.addEventListener("mouseup", handleMouseUp);
        },
        [bottomPanelHeight],
    );

    // --- Keyboard shortcuts ---
    useKeyboardShortcuts({
        binaryInfo,
        tabs,
        activeTabId,
        onOpenFile: handleOpenFile,
        onToggleBookmark: handleToggleBookmark,
        onUndo: handleUndo,
        onRedo: handleRedo,
        onGoBack: goBack,
        onGoForward: goForward,
        onOpenGoto: () => setGotoOpen(true),
        onOpenRename: openRename,
        onOpenComment: openComment,
        onToggleBottomPanel: handleToggleBottomPanel,
        onToggleDevTools: handleToggleDevTools,
    });

    // --- Native menu bar events ---
    useMenuEvents({
        onOpenFile: handleOpenFile,
        onSaveProject: handleSaveProject,
        onLoadProject: handleLoadProject,
        onSaveSnapshot: handleSaveSnapshot,
        onLoadSnapshot: handleLoadSnapshot,
        onExportJson: handleExportJson,
        onClearConsole: handleClearConsole,
        onClearCache: handleClearDecompileCache,
        onGotoAddress: () => { if (binaryInfo) setGotoOpen(true); },
        onRenameSymbol: () => {
            const tab = tabs.find((t) => t.id === activeTabId);
            if (tab) openRename(tab.address, tab.functionName);
        },
        onAddComment: () => {
            const tab = tabs.find((t) => t.id === activeTabId);
            if (tab) openComment(tab.address, "");
        },
        onDecompilerOptions: () => setDecompilerOptionsOpen(true),
        onToggleDynamic: handleToggleDynamicMode,
        onAssemblyView: handleOpenAssemblyView,
        onDecompileView: handleOpenDecompileView,
        onListingView: handleOpenListingTab,
        onToggleSidebar: handleToggleSidebar,
        onToggleBottom: handleToggleBottomPanel,
        onAbout: () => setAboutOpen(true),
    });

    // --- Drag & Drop ---
    useDragAndDrop({ log, onLoadBinary: handleLoadBinary });

    // --- Active tab ---
    const activeTab = tabs.find((t) => t.id === activeTabId) ?? null;

    // --- Render ---
    return (
        <div className="app-layout">
            <div className="app-body">
                <ActivityBar activeView={activeView} onViewChange={setActiveView} />

                {sidebarVisible && (
                <Sidebar title={
                    activeView === "settings" ? "Settings" :
                    activeView === "debug" ? "Debug" :
                    activeView === "search" ? "Search" :
                    activeView === "plugins" ? "Plugins" :
                    binaryInfo ? binaryInfo.name : "Explorer"
                }>
                    {activeView === "explorer" && (
                        <>
                            {binaryInfo && (
                                <div className="explorer__action-bar">
                                    <button
                                        className="explorer__action-btn"
                                        onClick={handleRunFid}
                                        disabled={fidRunning}
                                        title="Automatically identify library functions using the signature DB"
                                    >
                                        {fidRunning ? "Identifying…" : "🔍 Function ID (FID)"}
                                    </button>
                                </div>
                            )}
                            <FunctionsList
                                functions={functions}
                                loading={loading}
                                onFunctionClick={handleFunctionClick}
                                onOpenFile={handleOpenFile}
                                selectedAddress={activeTab?.address ?? null}
                                onRenameFunc={(func) => {
                                    openRename(func.address, func.name);
                                }}
                                onToggleBookmarkFunc={(func) =>
                                    handleToggleBookmarkAt(func.address, func.name)
                                }
                                onCopyAddress={(addr) => log(`Copied: ${addr}`)}
                                onOpenHex={handleOpenHexTab}
                            />
                            <SectionsPanel sections={sections} />
                        </>
                    )}
                    {activeView === "settings" && (
                        <SettingsPanel
                            settings={settings}
                            onSettingsChange={setSettings}
                            onLog={log}
                            onClearCache={handleClearDecompileCache}
                        />
                    )}
                    {activeView === "search" && (
                        <SearchPanel
                            binaryLoaded={!!binaryInfo}
                            onResultClick={handleNavigateToAddress}
                        />
                    )}
                    {activeView === "debug" && (
                        <DebugSidebar
                            onOpenDebugTab={() => setBottomTab("debug")}
                            onLog={log}
                        />
                    )}
                    {activeView === "plugins" && (
                        <PluginsPanel onLog={log} />
                    )}
                </Sidebar>
                )}

                <div className="main-area">
                    <EditorTabs
                        tabs={tabs}
                        activeTabId={activeTabId}
                        onTabClick={handleTabClick}
                        onTabClose={handleCloseTab}
                        canGoBack={canGoBack}
                        canGoForward={canGoForward}
                        onGoBack={goBack}
                        onGoForward={goForward}
                    />

                    <div className="editor-content">
                        {activeTab ? (
                            activeTab.type === "decompile" ? (
                                <DecompileView
                                    code={decompileCache[activeTab.id] ?? null}
                                    functionName={activeTab.functionName}
                                    onSymbolClick={(sym) => {
                                        // Try to navigate to matching function by name
                                        const matchByName = functions.find(
                                            (f) => f.name.toLowerCase() === sym.toLowerCase()
                                        );
                                        if (matchByName) {
                                            handleFunctionClick(matchByName);
                                            return;
                                        }
                                        // Try as hex address
                                        if (/^[0-9a-fA-F]{6,}$/.test(sym)) {
                                            handleNavigateToAddress(`0x${sym}`);
                                            return;
                                        }
                                        log(`Symbol: ${sym}`);
                                    }}
                                    onRename={(sym) => {
                                        // Find address of the symbol
                                        const matchByName = functions.find(
                                            (f) => f.name.toLowerCase() === sym.toLowerCase()
                                        );
                                        const targetAddr = matchByName?.address ?? activeTab.address;
                                        openRename(targetAddr, sym);
                                    }}
                                />
                            ) : activeTab.type === "assembly" ? (
                                <AssemblyView
                                    instructions={asmCache[activeTab.id] ?? null}
                                    onAddressClick={handleNavigateToAddress}
                                    onCommentEdit={(addr, comment) => {
                                        openComment(addr, comment);
                                    }}
                                    onRename={(addr, currentName) => {
                                        openRename(addr, currentName);
                                    }}
                                    onToggleBookmark={(addr) =>
                                        handleToggleBookmarkAt(addr, activeTab.functionName)
                                    }
                                    functionName={activeTab.functionName}
                                    selectedAddress={null}
                                    hasMore={asmHasMore[activeTab.id] ?? false}
                                    loadingMore={asmLoadingMore[activeTab.id] ?? false}
                                    onLoadMore={() => handleAsmLoadMore(activeTab.id, activeTab.address)}
                                />
                            ) : activeTab.type === "hexview" ? (
                                <HexView
                                    binaryLoaded={!!binaryInfo}
                                    initialAddress={activeTab.address}
                                    onLog={log}
                                    onPatchApplied={handleRecordPatch}
                                />
                            ) : activeTab.type === "listing" ? (
                                <ListingView
                                    binaryLoaded={!!binaryInfo}
                                    onLog={log}
                                />
                            ) : (
                                <div className="listing-placeholder">
                                    <div className="listing-placeholder__icon">🔢</div>
                                    <div className="listing-placeholder__title">Hex Editor</div>
                                    <div className="listing-placeholder__sub">Hex editor tab — coming soon</div>
                                </div>
                            )
                        ) : (
                            <div className="welcome">
                                <div className="welcome__title">Fission</div>
                                <div className="welcome__subtitle">Reverse Engineering Platform</div>
                                <div className="welcome__shortcuts">
                                    <button className="welcome__action" onClick={handleOpenFile}>
                                        Open Binary (Ctrl+O)
                                    </button>
                                    <div className="welcome__hint">or drag &amp; drop a file</div>
                                </div>
                                <div className="welcome__keys">
                                    <div><kbd>G</kbd> Go to Address</div>
                                    <div><kbd>N</kbd> Rename Symbol</div>
                                    <div><kbd>;</kbd> Add Comment</div>
                                    <div><kbd>F2</kbd> Toggle Bookmark</div>
                                    <div><kbd>Alt+←/→</kbd> Navigate History</div>
                                </div>
                            </div>
                        )}
                    </div>

                    {bottomPanelVisible && (
                        <>
                            <div
                                className={`resize-handle ${resizeRef.current ? "resize-handle--dragging" : ""}`}
                                onMouseDown={handleResizeStart}
                            />

                            <BottomPanel
                                activeTab={bottomTab}
                                onTabChange={setBottomTab}
                                height={bottomPanelHeight}
                        logs={logs}
                        strings={strings}
                        imports={imports}
                        bookmarks={bookmarks}
                        hexData={hexData}
                        xrefs={xrefs}
                        xrefAddress={activeTab?.address ?? null}
                        cfgAddress={activeTab?.type === "decompile" ? activeTab.address : null}
                        binaryLoaded={!!binaryInfo}
                        onNavigate={handleNavigateToAddress}
                        onLog={log}
                        functions={functions}
                        onClearConsole={handleClearConsole}
                        patches={patches}
                        onRevertPatch={handleRevertPatch}
                        dynamicMode={dynamicMode}
                        onUndo={handleUndo}
                        onRedo={handleRedo}
                        onExit={handleExit}
                        onLoadBinary={handleLoadBinary}
                    />
                        </>
                    )}
                </div>
            </div>

            <StatusBar
                binaryInfo={binaryInfo}
                functionCount={functions.length}
                gitBranch={gitBranch}
                progress={progress}
                dynamicMode={dynamicMode}
                onToggleDynamicMode={handleToggleDynamicMode}
            />

            {/* Dialogs */}
            <GotoDialog
                open={gotoOpen}
                onClose={() => setGotoOpen(false)}
                onGoto={handleNavigateToAddress}
            />
            <RenameDialog
                open={renameOpen}
                currentName={renameTarget.name}
                address={renameTarget.address}
                onClose={() => setRenameOpen(false)}
                onRename={handleRename}
            />
            <CommentDialog
                open={commentOpen}
                address={commentTarget.address}
                currentComment={commentTarget.comment}
                onClose={() => setCommentOpen(false)}
                onSave={handleAddComment}
            />
            <AboutDialog
                open={aboutOpen}
                onClose={() => setAboutOpen(false)}
            />
            <DecompilerOptionsDialog
                open={decompilerOptionsOpen}
                onClose={() => setDecompilerOptionsOpen(false)}
                onApplied={() => {
                    // Re-decompile the current function if one is open
                    const tab = tabs.find((t) => t.id === activeTabId);
                    if (tab) {
                        const addr = parseAddress(tab.address);
                        if (addr !== null) {
                            invoke("decompile_function", { address: addr }).catch(() => {});
                        }
                    }
                }}
                onLog={log}
            />
        </div>
    );
}

export default App;
