import { useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { parseAddress } from "../utils/address";
import { ASM_PAGE, DECOMPILE_TIMEOUT_MS, HEX_PREVIEW_SIZE } from "../utils/constants";
import type {
    BinaryInfo,
    FunctionDto,
    AsmInstructionDto,
    StringDto,
    ImportDto,
    BookmarkDto,
    HexViewData,
    XrefDto,
    SectionDto,
    PatchRecord,
    FidResultDto,
} from "../types";

interface UseBinaryOptions {
    log: (msg: string) => void;
    onOpenTabs: (func: FunctionDto, binaryInfo: BinaryInfo | null) => { decompTabId: string; asmTabId: string } | null;
    onResetTabs: () => void;
}

export function useBinary({ log, onOpenTabs, onResetTabs }: UseBinaryOptions) {
    const [binaryInfo, setBinaryInfo] = useState<BinaryInfo | null>(null);
    const [functions, setFunctions] = useState<FunctionDto[]>([]);
    const [sections, setSections] = useState<SectionDto[]>([]);
    const [strings, setStrings] = useState<StringDto[]>([]);
    const [imports, setImports] = useState<ImportDto[]>([]);
    const [bookmarks, setBookmarks] = useState<BookmarkDto[]>([]);
    const [patches, setPatches] = useState<PatchRecord[]>([]);
    const [hexData, setHexData] = useState<HexViewData | null>(null);
    const [xrefs, setXrefs] = useState<XrefDto[]>([]);
    const [loading, setLoading] = useState(false);
    const [progress, setProgress] = useState<{ value: number; message: string } | null>(null);
    const [fidRunning, setFidRunning] = useState(false);

    // Caches
    const [decompileCache, setDecompileCache] = useState<Record<string, string>>({});
    const [asmCache, setAsmCache] = useState<Record<string, AsmInstructionDto[]>>({});
    const [asmHasMore, setAsmHasMore] = useState<Record<string, boolean>>({});
    const [asmLoadingMore, setAsmLoadingMore] = useState<Record<string, boolean>>({});

    // --- Internal binary loader ---
    const _loadBinaryState = useCallback(
        async (path: string) => {
            const info = await invoke<BinaryInfo>("open_file", { path });
            setBinaryInfo(info);
            log(`Loaded: ${info.name} (${info.format} / ${info.arch})`);
            log(`  Functions: ${info.function_count}, Sections: ${info.section_count}`);
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
            invoke<SectionDto[]>("get_sections").then(setSections);
            invoke<BookmarkDto[]>("get_bookmarks").then(setBookmarks);
            return info;
        },
        [log],
    );

    // --- Open File Dialog ---
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
            const path = typeof selected === "string" ? selected : (selected as string);
            setLoading(true);
            log(`Loading: ${path}`);
            onResetTabs();
            setDecompileCache({});
            setAsmCache({});
            await _loadBinaryState(path);
        } catch (err) {
            log(`Error: ${err}`);
        } finally {
            setLoading(false);
        }
    }, [log, onResetTabs, _loadBinaryState]);

    // --- Load Binary by Path (from console) ---
    const handleLoadBinary = useCallback(
        async (path: string) => {
            setLoading(true);
            setProgress({ value: 0.1, message: `Loading ${path}...` });
            try {
                onResetTabs();
                setDecompileCache({});
                setAsmCache({});
                setProgress({ value: 0.5, message: "Loading functions..." });
                await _loadBinaryState(path);
                setProgress({ value: 0.9, message: "Loading metadata..." });
            } catch (err) {
                log(`Load binary error: ${err}`);
            } finally {
                setLoading(false);
                setProgress(null);
            }
        },
        [log, onResetTabs, _loadBinaryState],
    );

    // --- FID ---
    const handleRunFid = useCallback(async () => {
        setFidRunning(true);
        try {
            const result = await invoke<FidResultDto>("run_fid");
            log(`FID: ${result.matched} / ${result.total_scanned} functions identified`);
            const funcs = await invoke<FunctionDto[]>("get_functions");
            setFunctions(funcs);
            setAsmCache({});
        } catch (e) {
            log(`FID error: ${e}`);
        } finally {
            setFidRunning(false);
        }
    }, [log]);

    // --- Function Click (opens tabs + loads decompile/asm/hex/xrefs) ---
    const handleFunctionClick = useCallback(
        async (func: FunctionDto) => {
            const tabIds = onOpenTabs(func, binaryInfo);
            if (!tabIds) return;
            const { decompTabId, asmTabId } = tabIds;

            const addr = parseAddress(func.address);

            const decompPromise = (async () => {
                if (decompileCache[decompTabId]) return;
                try {
                    log(`Decompiling ${func.name}...`);
                    const timeout = new Promise<never>((_, reject) =>
                        setTimeout(() => reject(new Error("Decompile timeout (30s)")), DECOMPILE_TIMEOUT_MS),
                    );
                    const result = await Promise.race([
                        invoke<{ code: string }>("decompile_function", { address: addr }),
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
                        count: ASM_PAGE,
                    });
                    setAsmCache((prev) => ({ ...prev, [asmTabId]: instructions }));
                    setAsmHasMore((prev) => ({ ...prev, [asmTabId]: instructions.length === ASM_PAGE }));
                } catch (err) {
                    log(`Assembly error: ${err}`);
                }
            })();

            const hexPromise = (async () => {
                try {
                    const hex = await invoke<HexViewData>("get_hex_view", { address: addr, length: HEX_PREVIEW_SIZE });
                    setHexData(hex);
                } catch (_) {}
            })();

            const xrefPromise = (async () => {
                try {
                    const refs = await invoke<XrefDto[]>("get_xrefs", { address: addr });
                    setXrefs(refs);
                } catch (_) {}
            })();

            await Promise.allSettled([decompPromise, asmPromise, hexPromise, xrefPromise]);
        },
        [binaryInfo, decompileCache, asmCache, log, onOpenTabs],
    );

    // --- Load More Assembly ---
    const handleAsmLoadMore = useCallback(
        async (tabId: string, address: string) => {
            if (asmLoadingMore[tabId] || !asmHasMore[tabId]) return;
            setAsmLoadingMore((prev) => ({ ...prev, [tabId]: true }));
            try {
                const currentCount = asmCache[tabId]?.length ?? 0;
                const instructions = await invoke<AsmInstructionDto[]>("get_assembly", {
                    address,
                    count: currentCount + ASM_PAGE,
                });
                setAsmCache((prev) => ({ ...prev, [tabId]: instructions }));
                setAsmHasMore((prev) => ({
                    ...prev,
                    [tabId]: instructions.length === currentCount + ASM_PAGE,
                }));
            } catch (err) {
                log(`Assembly load-more error: ${err}`);
            } finally {
                setAsmLoadingMore((prev) => ({ ...prev, [tabId]: false }));
            }
        },
        [asmCache, asmHasMore, asmLoadingMore, log],
    );

    // --- Bookmarks ---
    const handleToggleBookmark = useCallback(
        async (address: string, label: string) => {
            try {
                const added = await invoke<boolean>("toggle_bookmark", { address, label });
                log(`Bookmark ${added ? "added" : "removed"}: ${address}`);
                const bms = await invoke<BookmarkDto[]>("get_bookmarks");
                setBookmarks(bms);
            } catch (err) {
                log(`Bookmark error: ${err}`);
            }
        },
        [log],
    );

    // --- Patches ---
    const handleRecordPatch = useCallback((rec: PatchRecord) => {
        setPatches((prev) => [...prev, rec]);
    }, []);

    const handleRevertPatch = useCallback(
        async (rec: PatchRecord) => {
            try {
                await invoke<number[]>("patch_bytes", { address: rec.address, bytes: rec.original });
                setPatches((prev) => prev.filter((p) => p !== rec));
                log(`Reverted patch at 0x${rec.address.toString(16)}`);
            } catch (err) {
                log(`Revert error: ${err}`);
            }
        },
        [log],
    );

    // --- Clear Cache ---
    const handleClearCache = useCallback(async () => {
        setDecompileCache({});
        setAsmCache({});
        await invoke("clear_decompiler_cache").catch(() => {});
        log("Decompile & assembly cache cleared.");
    }, [log]);

    const clearAsmCache = useCallback(() => setAsmCache({}), []);

    return {
        binaryInfo,
        setBinaryInfo,
        functions,
        setFunctions,
        sections,
        strings,
        imports,
        bookmarks,
        setBookmarks,
        patches,
        hexData,
        xrefs,
        loading,
        progress,
        fidRunning,
        decompileCache,
        asmCache,
        asmHasMore,
        asmLoadingMore,
        handleOpenFile,
        handleLoadBinary,
        handleRunFid,
        handleFunctionClick,
        handleAsmLoadMore,
        handleToggleBookmark,
        handleRecordPatch,
        handleRevertPatch,
        handleClearCache,
        clearAsmCache,
    };
}
