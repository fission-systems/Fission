import { useState, useCallback, useRef, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
    BinaryInfo,
    FunctionDto,
    EditorTab,
    ActivityView,
    BottomTab,
    DecompileResult,
    AsmInstructionDto,
    StringDto,
} from "./types";
import ActivityBar from "./components/ActivityBar";
import Sidebar from "./components/Sidebar";
import EditorTabs from "./components/EditorTabs";
import StatusBar from "./components/StatusBar";
import BottomPanel from "./components/BottomPanel";
import FunctionsList from "./panels/FunctionsList";
import DecompileView from "./panels/DecompileView";
import AssemblyView from "./panels/AssemblyView";

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
    const [loading, setLoading] = useState(false);

    // Decompile cache: tabId -> code
    const [decompileCache, setDecompileCache] = useState<Record<string, string>>({});
    const [asmCache, setAsmCache] = useState<Record<string, AsmInstructionDto[]>>({});

    const log = useCallback((msg: string) => {
        setLogs((prev) => [...prev, `[${new Date().toLocaleTimeString()}] ${msg}`]);
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

            const info = await invoke<BinaryInfo>("open_file", { path });
            setBinaryInfo(info);
            log(`Loaded: ${info.name} (${info.format} / ${info.arch})`);
            log(`  Functions: ${info.function_count}, Sections: ${info.section_count}`);

            const funcs = await invoke<FunctionDto[]>("get_functions");
            setFunctions(funcs);
            log(`  Found ${funcs.length} functions`);

            // Load strings in background
            invoke<StringDto[]>("get_strings").then((s) => {
                setStrings(s);
                log(`  Extracted ${s.length} strings`);
            });
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

            // Add tabs if not already open
            setTabs((prev) => {
                const existing = prev.find((t) => t.id === decompTabId);
                if (existing) return prev;
                return [
                    ...prev,
                    {
                        id: decompTabId,
                        title: func.name,
                        type: "decompile",
                        address: func.address,
                        functionName: func.name,
                    },
                    {
                        id: asmTabId,
                        title: `${func.name} [ASM]`,
                        type: "assembly",
                        address: func.address,
                        functionName: func.name,
                    },
                ];
            });
            setActiveTabId(decompTabId);

            // Decompile if not cached
            if (!decompileCache[decompTabId]) {
                try {
                    log(`Decompiling ${func.name}...`);
                    const addr = parseInt(func.address, 16) || parseInt(func.address);
                    const result = await invoke<DecompileResult>("decompile_function", { address: addr });
                    setDecompileCache((prev) => ({ ...prev, [decompTabId]: result.code }));
                    log(`Decompiled ${func.name} ✓`);
                } catch (err) {
                    const errMsg = `// Error decompiling ${func.name}: ${err}`;
                    setDecompileCache((prev) => ({ ...prev, [decompTabId]: errMsg }));
                    log(`Decompile error: ${err}`);
                }
            }

            // Disassemble if not cached
            if (!asmCache[asmTabId]) {
                try {
                    const addr = parseInt(func.address, 16) || parseInt(func.address);
                    const instructions = await invoke<AsmInstructionDto[]>("get_assembly", {
                        address: addr,
                        count: 200,
                    });
                    setAsmCache((prev) => ({ ...prev, [asmTabId]: instructions }));
                } catch (err) {
                    log(`Assembly error: ${err}`);
                }
            }
        },
        [decompileCache, asmCache, log],
    );

    // --- Tab management ---
    const handleCloseTab = useCallback(
        (tabId: string) => {
            setTabs((prev) => prev.filter((t) => t.id !== tabId));
            if (activeTabId === tabId) {
                setTabs((prev) => {
                    const remaining = prev.filter((t) => t.id !== tabId);
                    setActiveTabId(remaining.length > 0 ? remaining[remaining.length - 1].id : null);
                    return remaining;
                });
            }
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
            if (e.ctrlKey && e.key === "o") {
                e.preventDefault();
                handleOpenFile();
            }
        };
        window.addEventListener("keydown", handleKeyDown);
        return () => window.removeEventListener("keydown", handleKeyDown);
    }, [handleOpenFile]);

    // --- Active tab ---
    const activeTab = tabs.find((t) => t.id === activeTabId) ?? null;

    // --- Render ---
    return (
        <div className="app-layout">
            <ActivityBar activeView={activeView} onViewChange={setActiveView} />

            <Sidebar title={binaryInfo ? binaryInfo.name : "Explorer"}>
                {activeView === "explorer" && (
                    <FunctionsList
                        functions={functions}
                        loading={loading}
                        onFunctionClick={handleFunctionClick}
                        onOpenFile={handleOpenFile}
                        selectedAddress={activeTab?.address ?? null}
                    />
                )}
            </Sidebar>

            <div className="main-area">
                <EditorTabs
                    tabs={tabs}
                    activeTabId={activeTabId}
                    onTabClick={setActiveTabId}
                    onTabClose={handleCloseTab}
                />

                <div className="editor-content">
                    {activeTab ? (
                        activeTab.type === "decompile" ? (
                            <DecompileView code={decompileCache[activeTab.id] ?? null} />
                        ) : (
                            <AssemblyView instructions={asmCache[activeTab.id] ?? null} />
                        )
                    ) : (
                        <div className="welcome">
                            <div className="welcome__title">Fission</div>
                            <div className="welcome__subtitle">Reverse Engineering Platform</div>
                            <button className="welcome__action" onClick={handleOpenFile}>
                                Open Binary (Ctrl+O)
                            </button>
                        </div>
                    )}
                </div>

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
                />
            </div>

            <StatusBar binaryInfo={binaryInfo} functionCount={functions.length} />
        </div>
    );
}

export default App;
