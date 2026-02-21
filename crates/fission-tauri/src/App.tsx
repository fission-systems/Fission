import { useState, useCallback, useRef, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open, save } from "@tauri-apps/plugin-dialog";
import type {
    BinaryInfo,
    FunctionDto,
    EditorTab,
    ActivityView,
    BottomTab,
    DecompileResult,
    AsmInstructionDto,
    StringDto,
    ImportDto,
    BookmarkDto,
    GotoResult,
    HexViewData,
    XrefDto,
    AppSettings,
    SectionDto,
    PatchRecord,
} from "./types";
import { ListingView } from "./panels/ListingView";
import { DebugSidebar } from "./panels/DebugSidebar";
import MenuBar from "./components/MenuBar";
import ActivityBar from "./components/ActivityBar";
import Sidebar from "./components/Sidebar";
import EditorTabs from "./components/EditorTabs";
import StatusBar from "./components/StatusBar";
import BottomPanel from "./components/BottomPanel";
import GotoDialog from "./components/GotoDialog";
import RenameDialog from "./components/RenameDialog";
import CommentDialog from "./components/CommentDialog";
import FunctionsList from "./panels/FunctionsList";
import DecompileView from "./panels/DecompileView";
import AssemblyView from "./panels/AssemblyView";
import HexView from "./panels/HexView";
import SettingsPanel from "./panels/SettingsPanel";
import SectionsPanel from "./panels/SectionsPanel";
import AboutDialog from "./components/AboutDialog";
import SearchPanel from "./panels/SearchPanel";

function App() {
    // --- State ---
    const [binaryInfo, setBinaryInfo] = useState<BinaryInfo | null>(null);
    const [functions, setFunctions] = useState<FunctionDto[]>([]);
    const [activeView, setActiveView] = useState<ActivityView>("explorer");
    const [tabs, setTabs] = useState<EditorTab[]>([]);
    const [activeTabId, setActiveTabId] = useState<string | null>(null);
    const [bottomTab, setBottomTab] = useState<BottomTab>("console");
    const [bottomPanelHeight, setBottomPanelHeight] = useState(200);
    const [logs, setLogs] = useState<string[]>(["[Fission] Ready."]);
    const [strings, setStrings] = useState<StringDto[]>([]);
    const [imports, setImports] = useState<ImportDto[]>([]);
    const [bookmarks, setBookmarks] = useState<BookmarkDto[]>([]);
    const [hexData, setHexData] = useState<HexViewData | null>(null);
    const [xrefs, setXrefs] = useState<XrefDto[]>([]);
    const [loading, setLoading] = useState(false);
    const [sections, setSections] = useState<SectionDto[]>([]);
    const [bottomPanelVisible, setBottomPanelVisible] = useState(true);
    const [aboutOpen, setAboutOpen] = useState(false);
    const [patches, setPatches] = useState<PatchRecord[]>([]);

    // Caches
    const [decompileCache, setDecompileCache] = useState<Record<string, string>>({});
    const [asmCache, setAsmCache] = useState<Record<string, AsmInstructionDto[]>>({});

    // Navigation history
    const historyStack = useRef<string[]>([]);
    const historyIndex = useRef(-1);
    const navigatingRef = useRef(false); // prevent push during back/forward

    // Settings
    const [settings, setSettings] = useState<AppSettings>({
        theme: "dark",
        font_size: 14,
        decompile_style: "c-like",
        simplify_level: 1,
    });

    // Dialog state
    const [gotoOpen, setGotoOpen] = useState(false);
    const [renameOpen, setRenameOpen] = useState(false);
    const [renameTarget, setRenameTarget] = useState({ address: "", name: "" });
    const [commentOpen, setCommentOpen] = useState(false);
    const [commentTarget, setCommentTarget] = useState({ address: "", comment: "" });

    // Undo/Redo stacks
    interface UndoableAction {
        type: "rename" | "comment";
        address: string;
        previousValue: string;
        newValue: string;
    }
    const undoStack = useRef<UndoableAction[]>([]);
    const redoStack = useRef<UndoableAction[]>([]);

    const log = useCallback((msg: string) => {
        setLogs((prev) => [...prev, `[${new Date().toLocaleTimeString()}] ${msg}`]);
    }, []);

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
            interface FissionProject {
                binary_path: string;
                comments: Record<string, string>;
                renames: Record<string, string>;
                bookmarks: BookmarkDto[];
            }
            const project = await invoke<FissionProject>("load_project", { path });
            log(`Project loaded from: ${path}`);

            // If a binary is already loaded and paths differ, reload it
            if (!binaryInfo || binaryInfo.path !== project.binary_path) {
                log(`Reloading binary: ${project.binary_path}`);
                setLoading(true);
                setTabs([]);
                setActiveTabId(null);
                setDecompileCache({});
                setAsmCache({});
                historyStack.current = [];
                historyIndex.current = -1;
                try {
                    const info = await invoke<BinaryInfo>("open_file", { path: project.binary_path });
                    setBinaryInfo(info);
                    log(`Loaded: ${info.name} (${info.format} / ${info.arch})`);
                    const funcs = await invoke<FunctionDto[]>("get_functions");
                    setFunctions(funcs);
                    invoke<StringDto[]>("get_strings").then(setStrings);
                    invoke<ImportDto[]>("get_imports").then(setImports);
                    invoke<SectionDto[]>("get_sections").then(setSections);
                } catch (binErr) {
                    log(`Binary reload error: ${binErr}`);
                } finally {
                    setLoading(false);
                }
            }

            // Reload bookmarks and function list from restored state
            invoke<BookmarkDto[]>("get_bookmarks").then(setBookmarks);
            invoke<FunctionDto[]>("get_functions").then(setFunctions);
            log("Annotations restored (comments, renames, bookmarks).");
        } catch (err) {
            log(`Load project error: ${err}`);
        }
    }, [binaryInfo, log]);

    // --- Clear Console ---
    const handleClearConsole = useCallback(() => {
        setLogs([]);
    }, []);

    // --- Exit ---
    const handleExit = useCallback(() => {
        getCurrentWindow().close();
    }, []);

    // --- Toggle Bottom Panel ---
    const handleToggleBottomPanel = useCallback(() => {
        setBottomPanelVisible((v) => !v);
    }, []);

    // --- Clear Decompile Cache ---
    const handleClearDecompileCache = useCallback(async () => {
        setDecompileCache({});
        setAsmCache({});
        await invoke("clear_decompiler_cache").catch(() => {});
        log("Decompile & assembly cache cleared.");
    }, [log]);

    // --- Open Listing Tab ---
    const handleOpenListingTab = useCallback(() => {
        if (!binaryInfo) return;
        const tabId = "listing-main";
        setTabs((prev) => {
            if (prev.find((t) => t.id === tabId)) return prev;
            return [
                ...prev,
                {
                    id: tabId,
                    title: "Listing",
                    type: "listing" as const,
                    address: "0x0",
                    functionName: "Listing",
                },
            ];
        });
        setActiveTabId(tabId);
    }, [binaryInfo]);

    // --- Open Hex Editor Tab ---
    const handleOpenHexTab = useCallback((func: FunctionDto) => {
        if (!binaryInfo) return;
        const tabId = `hex-${func.address}`;
        setTabs((prev) => {
            if (prev.find((t) => t.id === tabId)) return prev;
            return [
                ...prev,
                {
                    id: tabId,
                    title: `${func.name} [HEX]`,
                    type: "hexview" as const,
                    address: func.address,
                    functionName: func.name,
                },
            ];
        });
        setActiveTabId(tabId);
    }, [binaryInfo]);

    // --- Navigation helpers ---
    const pushHistory = useCallback((tabId: string) => {
        if (navigatingRef.current) return;
        const stack = historyStack.current;
        const idx = historyIndex.current;
        // Trim forward history
        historyStack.current = stack.slice(0, idx + 1);
        historyStack.current.push(tabId);
        historyIndex.current = historyStack.current.length - 1;
    }, []);

    const canGoBack = historyIndex.current > 0;
    const canGoForward = historyIndex.current < historyStack.current.length - 1;

    const goBack = useCallback(() => {
        if (historyIndex.current > 0) {
            navigatingRef.current = true;
            historyIndex.current--;
            setActiveTabId(historyStack.current[historyIndex.current]);
            navigatingRef.current = false;
        }
    }, []);

    const goForward = useCallback(() => {
        if (historyIndex.current < historyStack.current.length - 1) {
            navigatingRef.current = true;
            historyIndex.current++;
            setActiveTabId(historyStack.current[historyIndex.current]);
            navigatingRef.current = false;
        }
    }, []);

    // --- Load settings on startup ---
    useEffect(() => {
        invoke<AppSettings>("get_settings")
            .then(setSettings)
            .catch((e) => console.warn("get_settings failed:", e));
    }, []);

    // --- File Open ---
    const handleOpenFile = useCallback(async () => {
        try {
            const selected = await open({
                multiple: false,
                filters: [
                    { name: "Executables", extensions: ["exe", "dll", "elf", "so", "dylib", "bin", "o"] },
                    { name: "All Files", extensions: ["*"] },
                ],
            });
            if (!selected) return;

            const path = typeof selected === "string" ? selected : selected;
            setLoading(true);
            log(`Loading: ${path}`);

            // Reset state
            setTabs([]);
            setActiveTabId(null);
            setDecompileCache({});
            setAsmCache({});
            historyStack.current = [];
            historyIndex.current = -1;

            const info = await invoke<BinaryInfo>("open_file", { path });
            setBinaryInfo(info);
            log(`Loaded: ${info.name} (${info.format} / ${info.arch})`);
            log(`  Functions: ${info.function_count}, Sections: ${info.section_count}`);

            const funcs = await invoke<FunctionDto[]>("get_functions");
            setFunctions(funcs);
            log(`  Found ${funcs.length} functions`);

            // Load strings and imports in background
            invoke<StringDto[]>("get_strings").then((s) => {
                setStrings(s);
                log(`  Extracted ${s.length} strings`);
            });
            invoke<ImportDto[]>("get_imports").then((imp) => {
                setImports(imp);
                log(`  Found ${imp.length} imports`);
            });
            invoke<SectionDto[]>("get_sections").then(setSections);

            // Load bookmarks
            invoke<BookmarkDto[]>("get_bookmarks").then(setBookmarks);
        } catch (err) {
            log(`Error: ${err}`);
        } finally {
            setLoading(false);
        }
    }, [log]);

    // --- Function Click → Open tabs ---
    const handleFunctionClick = useCallback(
        async (func: FunctionDto) => {
            const decompTabId = `decomp-${func.address}`;
            const asmTabId = `asm-${func.address}`;

            setTabs((prev) => {
                const existing = prev.find((t) => t.id === decompTabId);
                if (existing) return prev;
                return [
                    ...prev,
                    {
                        id: decompTabId,
                        title: func.name,
                        type: "decompile" as const,
                        address: func.address,
                        functionName: func.name,
                    },
                    {
                        id: asmTabId,
                        title: `${func.name} [ASM]`,
                        type: "assembly" as const,
                        address: func.address,
                        functionName: func.name,
                    },
                ];
            });
            setActiveTabId(decompTabId);
            pushHistory(decompTabId);

            const addr = parseInt(func.address, 16) || parseInt(func.address);

            // Launch all loads in PARALLEL so one hanging doesn't block others
            const decompPromise = (async () => {
                if (decompileCache[decompTabId]) return;
                try {
                    log(`Decompiling ${func.name}...`);
                    // Race against a 30s timeout to prevent indefinite hangs
                    const timeout = new Promise<never>((_, reject) =>
                        setTimeout(() => reject(new Error("Decompile timeout (30s)")), 30000),
                    );
                    const result = await Promise.race([
                        invoke<DecompileResult>("decompile_function", { address: addr }),
                        timeout,
                    ]);
                    setDecompileCache((prev) => ({ ...prev, [decompTabId]: result.code }));
                    log(`Decompiled ${func.name} ✓`);
                } catch (err) {
                    const errMsg = `// Error decompiling ${func.name}: ${err}`;
                    setDecompileCache((prev) => ({ ...prev, [decompTabId]: errMsg }));
                    log(`Decompile error: ${err}`);
                }
            })();

            const asmPromise = (async () => {
                if (asmCache[asmTabId]) return;
                try {
                    const instructions = await invoke<AsmInstructionDto[]>("get_assembly", {
                        address: addr,
                        count: 200,
                    });
                    setAsmCache((prev) => ({ ...prev, [asmTabId]: instructions }));
                } catch (err) {
                    log(`Assembly error: ${err}`);
                }
            })();

            const hexPromise = (async () => {
                try {
                    const hex = await invoke<HexViewData>("get_hex_view", { address: addr, length: 256 });
                    setHexData(hex);
                } catch (_) { /* ignore hex errors */ }
            })();

            const xrefPromise = (async () => {
                try {
                    const refs = await invoke<XrefDto[]>("get_xrefs", { address: addr });
                    setXrefs(refs);
                } catch (_) { /* ignore xref errors */ }
            })();

            await Promise.allSettled([decompPromise, asmPromise, hexPromise, xrefPromise]);
        },
        [decompileCache, asmCache, log, pushHistory],
    );

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

    // --- Rename ---
    const handleRename = useCallback(
        async (address: string, newName: string) => {
            try {
                const addr = parseInt(address, 16) || parseInt(address);
                // Record previous name for undo
                const prevFunc = functions.find((f) => f.address.toLowerCase() === address.toLowerCase());
                const prevName = prevFunc?.name ?? "";
                await invoke("rename_function", { address: addr, newName });
                log(`Renamed: ${address} → ${newName || "(reverted)"}`);

                // Push to undo stack
                undoStack.current.push({ type: "rename", address, previousValue: prevName, newValue: newName });
                redoStack.current = [];

                // Refresh functions
                const funcs = await invoke<FunctionDto[]>("get_functions");
                setFunctions(funcs);

                // Update tabs with new name
                if (newName) {
                    setTabs((prev) =>
                        prev.map((t) => {
                            if (t.address === address) {
                                return {
                                    ...t,
                                    title: t.type === "assembly" ? `${newName} [ASM]` : newName,
                                    functionName: newName,
                                };
                            }
                            return t;
                        }),
                    );
                }
            } catch (err) {
                log(`Rename error: ${err}`);
            }
        },
        [log, functions],
    );

    // --- Comment ---
    const handleAddComment = useCallback(
        async (address: string, text: string) => {
            try {
                const addr = parseInt(address, 16) || parseInt(address);
                await invoke("add_comment", { address: addr, text });
                log(`Comment at ${address}: ${text || "(removed)"}`);

                // Push to undo stack
                undoStack.current.push({ type: "comment", address, previousValue: "", newValue: text });
                redoStack.current = [];

                // Refresh assembly for all tabs that might show this address
                // (simplified: invalidate all asm caches)
                setAsmCache({});
            } catch (err) {
                log(`Comment error: ${err}`);
            }
        },
        [log],
    );

    // --- Undo / Redo ---
    const handleUndo = useCallback(async () => {
        const action = undoStack.current.pop();
        if (!action) { log("Nothing to undo."); return; }
        try {
            if (action.type === "rename") {
                const addr = parseInt(action.address, 16) || parseInt(action.address);
                await invoke("rename_function", { address: addr, newName: action.previousValue });
                const funcs = await invoke<FunctionDto[]>("get_functions");
                setFunctions(funcs);
                setTabs((prev) => prev.map((t) =>
                    t.address === action.address
                        ? { ...t, title: t.type === "assembly" ? `${action.previousValue} [ASM]` : action.previousValue, functionName: action.previousValue }
                        : t
                ));
                log(`Undo rename: ${action.address} → ${action.previousValue}`);
            } else if (action.type === "comment") {
                const addr = parseInt(action.address, 16) || parseInt(action.address);
                await invoke("add_comment", { address: addr, text: action.previousValue });
                setAsmCache({});
                log(`Undo comment at ${action.address}`);
            }
            redoStack.current.push(action);
        } catch (err) {
            log(`Undo error: ${err}`);
        }
    }, [log]);

    const handleRedo = useCallback(async () => {
        const action = redoStack.current.pop();
        if (!action) { log("Nothing to redo."); return; }
        try {
            if (action.type === "rename") {
                const addr = parseInt(action.address, 16) || parseInt(action.address);
                await invoke("rename_function", { address: addr, newName: action.newValue });
                const funcs = await invoke<FunctionDto[]>("get_functions");
                setFunctions(funcs);
                setTabs((prev) => prev.map((t) =>
                    t.address === action.address
                        ? { ...t, title: t.type === "assembly" ? `${action.newValue} [ASM]` : action.newValue, functionName: action.newValue }
                        : t
                ));
                log(`Redo rename: ${action.address} → ${action.newValue}`);
            } else if (action.type === "comment") {
                const addr = parseInt(action.address, 16) || parseInt(action.address);
                await invoke("add_comment", { address: addr, text: action.newValue });
                setAsmCache({});
                log(`Redo comment at ${action.address}`);
            }
            undoStack.current.push(action);
        } catch (err) {
            log(`Redo error: ${err}`);
        }
    }, [log]);

    // --- Bookmark (active tab) ---
    const handleToggleBookmark = useCallback(async () => {
        const tab = tabs.find((t) => t.id === activeTabId);
        if (!tab) return;

        try {
            const added = await invoke<boolean>("toggle_bookmark", {
                address: tab.address,
                label: tab.functionName,
            });
            log(`Bookmark ${added ? "added" : "removed"}: ${tab.address}`);

            const bms = await invoke<BookmarkDto[]>("get_bookmarks");
            setBookmarks(bms);
        } catch (err) {
            log(`Bookmark error: ${err}`);
        }
    }, [tabs, activeTabId, log]);

    // --- Bookmark at specific address (from context menus) ---
    const handleToggleBookmarkAt = useCallback(async (address: string, label: string) => {
        try {
            const added = await invoke<boolean>("toggle_bookmark", { address, label });
            log(`Bookmark ${added ? "added" : "removed"}: ${address}`);
            const bms = await invoke<BookmarkDto[]>("get_bookmarks");
            setBookmarks(bms);
        } catch (err) {
            log(`Bookmark error: ${err}`);
        }
    }, [log]);

    // --- Patch management ---
    const handleRecordPatch = useCallback((rec: PatchRecord) => {
        setPatches((prev) => [...prev, rec]);
    }, []);

    const handleRevertPatch = useCallback(async (rec: PatchRecord) => {
        try {
            await invoke<number[]>("patch_bytes", { address: rec.address, bytes: rec.original });
            setPatches((prev) => prev.filter((p) => p !== rec));
            log(`Reverted patch at 0x${rec.address.toString(16)}`);
        } catch (err) {
            log(`Revert error: ${err}`);
        }
    }, [log]);

    // --- Tab management ---
    const handleTabClick = useCallback((tabId: string) => {
        setActiveTabId(tabId);
        pushHistory(tabId);
    }, [pushHistory]);

    const handleCloseTab = useCallback(
        (tabId: string) => {
            setTabs((prev) => {
                const remaining = prev.filter((t) => t.id !== tabId);
                if (activeTabId === tabId) {
                    setActiveTabId(remaining.length > 0 ? remaining[remaining.length - 1].id : null);
                }
                return remaining;
            });
        },
        [activeTabId],
    );

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
    useEffect(() => {
        const handleKeyDown = (e: KeyboardEvent) => {
            // Ignore when typing in an input
            if ((e.target as HTMLElement).tagName === "INPUT" || (e.target as HTMLElement).tagName === "TEXTAREA") return;

            if (e.ctrlKey && e.key === "o") {
                e.preventDefault();
                handleOpenFile();
                return;
            }

            // G: Go to address
            if (e.key === "g" && !e.ctrlKey && !e.altKey && binaryInfo) {
                e.preventDefault();
                setGotoOpen(true);
                return;
            }

            // Ctrl+Z: Undo
            if (e.ctrlKey && e.key === "z" && !e.shiftKey) {
                e.preventDefault();
                handleUndo();
                return;
            }

            // Ctrl+Y or Ctrl+Shift+Z: Redo
            if ((e.ctrlKey && e.key === "y") || (e.ctrlKey && e.shiftKey && e.key === "z")) {
                e.preventDefault();
                handleRedo();
                return;
            }

            // N: Rename
            if (e.key === "n" && !e.ctrlKey && !e.altKey) {
                e.preventDefault();
                const tab = tabs.find((t) => t.id === activeTabId);
                if (tab) {
                    setRenameTarget({ address: tab.address, name: tab.functionName });
                    setRenameOpen(true);
                }
                return;
            }

            // ;: Comment
            if (e.key === ";" && !e.ctrlKey && !e.altKey) {
                e.preventDefault();
                const tab = tabs.find((t) => t.id === activeTabId);
                if (tab) {
                    setCommentTarget({ address: tab.address, comment: "" });
                    setCommentOpen(true);
                }
                return;
            }

            // F2: Bookmark
            if (e.key === "F2") {
                e.preventDefault();
                handleToggleBookmark();
                return;
            }

            // Alt+Left: Back
            if (e.altKey && e.key === "ArrowLeft") {
                e.preventDefault();
                goBack();
                return;
            }

            // Alt+Right: Forward
            if (e.altKey && e.key === "ArrowRight") {
                e.preventDefault();
                goForward();
                return;
            }

            // Ctrl+J: Toggle bottom panel
            if (e.ctrlKey && e.key === "j") {
                e.preventDefault();
                setBottomPanelVisible((v) => !v);
                return;
            }
        };

        window.addEventListener("keydown", handleKeyDown);
        return () => window.removeEventListener("keydown", handleKeyDown);
    }, [handleOpenFile, handleToggleBookmark, handleUndo, handleRedo, goBack, goForward, binaryInfo, tabs, activeTabId]);

    // --- Drag & Drop ---
    useEffect(() => {
        const handleDragOver = (e: DragEvent) => {
            e.preventDefault();
            e.stopPropagation();
        };

        const handleDrop = async (e: DragEvent) => {
            e.preventDefault();
            e.stopPropagation();

            const files = e.dataTransfer?.files;
            if (files && files.length > 0) {
                const path = (files[0] as any).path;
                if (path) {
                    setLoading(true);
                    log(`Loading (dropped): ${path}`);
                    try {
                        setTabs([]);
                        setActiveTabId(null);
                        setDecompileCache({});
                        setAsmCache({});
                        historyStack.current = [];
                        historyIndex.current = -1;

                        const info = await invoke<BinaryInfo>("open_file", { path });
                        setBinaryInfo(info);
                        log(`Loaded: ${info.name} (${info.format} / ${info.arch})`);

                        const funcs = await invoke<FunctionDto[]>("get_functions");
                        setFunctions(funcs);
                        log(`  Found ${funcs.length} functions`);

                        invoke<StringDto[]>("get_strings").then((s) => {
                            setStrings(s);
                            log(`  Extracted ${s.length} strings`);
                        });
                        invoke<ImportDto[]>("get_imports").then((imp) => {
                            setImports(imp);
                            log(`  Found ${imp.length} imports`);
                        });
                        invoke<BookmarkDto[]>("get_bookmarks").then(setBookmarks);
                    } catch (err) {
                        log(`Error: ${err}`);
                    } finally {
                        setLoading(false);
                    }
                }
            }
        };

        document.addEventListener("dragover", handleDragOver);
        document.addEventListener("drop", handleDrop);
        return () => {
            document.removeEventListener("dragover", handleDragOver);
            document.removeEventListener("drop", handleDrop);
        };
    }, [log]);

    // --- Active tab ---
    const activeTab = tabs.find((t) => t.id === activeTabId) ?? null;

    // --- Render ---
    return (
        <div className="app-layout">
            <MenuBar
                onOpenFile={handleOpenFile}
                onSaveProject={handleSaveProject}
                onLoadProject={handleLoadProject}
                onClearConsole={handleClearConsole}
                onClearCache={handleClearDecompileCache}
                onOpenListing={handleOpenListingTab}
                onGotoAddress={() => binaryInfo && setGotoOpen(true)}
                onRename={() => {
                    const tab = tabs.find((t) => t.id === activeTabId);
                    if (tab) {
                        setRenameTarget({ address: tab.address, name: tab.functionName });
                        setRenameOpen(true);
                    }
                }}
                onComment={() => {
                    const tab = tabs.find((t) => t.id === activeTabId);
                    if (tab) {
                        setCommentTarget({ address: tab.address, comment: "" });
                        setCommentOpen(true);
                    }
                }}
                binaryLoaded={!!binaryInfo}
                onExit={handleExit}
                onToggleBottomPanel={handleToggleBottomPanel}
                bottomPanelVisible={bottomPanelVisible}
                onAbout={() => setAboutOpen(true)}
            />

            <div className="app-body">
                <ActivityBar activeView={activeView} onViewChange={setActiveView} />

                <Sidebar title={
                    activeView === "settings" ? "Settings" :
                    activeView === "debug" ? "Debug" :
                    activeView === "search" ? "Search" :
                    binaryInfo ? binaryInfo.name : "Explorer"
                }>
                    {activeView === "explorer" && (
                        <>
                            <FunctionsList
                                functions={functions}
                                loading={loading}
                                onFunctionClick={handleFunctionClick}
                                onOpenFile={handleOpenFile}
                                selectedAddress={activeTab?.address ?? null}
                                onRenameFunc={(func) => {
                                    setRenameTarget({ address: func.address, name: func.name });
                                    setRenameOpen(true);
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
                </Sidebar>

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
                                        setRenameTarget({ address: targetAddr, name: sym });
                                        setRenameOpen(true);
                                    }}
                                />
                            ) : activeTab.type === "assembly" ? (
                                <AssemblyView
                                    instructions={asmCache[activeTab.id] ?? null}
                                    onAddressClick={handleNavigateToAddress}
                                    onCommentEdit={(addr, comment) => {
                                        setCommentTarget({ address: addr, comment });
                                        setCommentOpen(true);
                                    }}
                                    onRename={(addr, currentName) => {
                                        setRenameTarget({ address: addr, name: currentName });
                                        setRenameOpen(true);
                                    }}
                                    onToggleBookmark={(addr) =>
                                        handleToggleBookmarkAt(addr, activeTab.functionName)
                                    }
                                    functionName={activeTab.functionName}
                                    selectedAddress={null}
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
                        onBookmarkClick={handleNavigateToAddress}
                        onImportClick={handleNavigateToAddress}
                        onStringClick={(offset) => log(`String at ${offset}`)}
                        onSearchResultClick={handleNavigateToAddress}
                        onXrefClick={handleNavigateToAddress}
                        onLog={log}
                        functions={functions}
                        onGotoAddress={handleNavigateToAddress}
                        onClearConsole={handleClearConsole}
                        patches={patches}
                        onRevertPatch={handleRevertPatch}
                        onExportClick={handleNavigateToAddress}
                        onNoteClick={handleNavigateToAddress}
                    />
                        </>
                    )}
                </div>
            </div>

            <StatusBar binaryInfo={binaryInfo} functionCount={functions.length} />

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
        </div>
    );
}

export default App;
