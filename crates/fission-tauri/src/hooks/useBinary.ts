import { useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { parseAddress } from "../utils/address";
import { ASM_PAGE, DECOMPILE_TIMEOUT_MS, HEX_PREVIEW_SIZE } from "../utils/constants";
import type {
    BinaryInfo,
    FunctionDto,
    DecompileResult,
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
    const [lastFidResult, setLastFidResult] = useState<FidResultDto | null>(null);

    // Caches
    const [decompileCache, setDecompileCache] = useState<Record<string, DecompileResult>>({});
    const [decompileStatus, setDecompileStatus] = useState<Record<string, "idle" | "loading" | "ready" | "error">>({});
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

            // Auto-run FID so identified names are available before first decompile.
            try {
                setFidRunning(true);
                const fidResult = await invoke<FidResultDto>("run_fid");
                setLastFidResult(fidResult);
                log(
                    `Auto FID: ${fidResult.matched} / ${fidResult.total_scanned} functions identified`,
                );
                log(
                    `Auto FID DB: attempted=${fidResult.fidbf_attempted}, loaded=${fidResult.fidbf_loaded}, failed=${fidResult.fidbf_failed}`,
                );
                const refreshed = await invoke<FunctionDto[]>("get_functions");
                setFunctions(refreshed);
            } catch (err) {
                log(`Auto FID skipped: ${err}`);
            } finally {
                setFidRunning(false);
            }

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
            setDecompileStatus({});
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
                setDecompileStatus({});
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
            setLastFidResult(result);
            log(`FID: ${result.matched} / ${result.total_scanned} functions identified`);
            log(
                `FID DB: attempted=${result.fidbf_attempted}, loaded=${result.fidbf_loaded}, failed=${result.fidbf_failed}`,
            );
            const funcs = await invoke<FunctionDto[]>("get_functions");
            setFunctions(funcs);
            setAsmCache({});
        } catch (e) {
            log(`FID error: ${e}`);
        } finally {
            setFidRunning(false);
        }
    }, [log]);

    const fetchDecompileResult = useCallback(
        async (address: number, tabId: string, functionName: string, addressText: string) => {
            setDecompileStatus((prev) => ({ ...prev, [tabId]: "loading" }));
            try {
                log(`Decompiling ${functionName}...`);
                const timeout = new Promise<never>((_, reject) =>
                    setTimeout(() => reject(new Error("Decompile timeout (30s)")), DECOMPILE_TIMEOUT_MS),
                );
                const result = await Promise.race([
                    invoke<DecompileResult>("decompile_function", { address }),
                    timeout,
                ]);
                setDecompileCache((prev) => ({ ...prev, [tabId]: result }));
                setDecompileStatus((prev) => ({ ...prev, [tabId]: "ready" }));
                log(`Decompiled ${functionName} ✓`);
                return result;
            } catch (err) {
                const fallback: DecompileResult = {
                    code: `// Error decompiling ${functionName}: ${err}`,
                    function_name: functionName,
                    address: addressText,
                    engine_used: "legacy",
                    fell_back: false,
                    fallback_reason: String(err),
                };
                setDecompileCache((prev) => ({ ...prev, [tabId]: fallback }));
                setDecompileStatus((prev) => ({ ...prev, [tabId]: "error" }));
                log(`Decompile error: ${err}`);
                return fallback;
            }
        },
        [log],
    );

    // --- Function Click (opens tabs + loads decompile/asm/hex/xrefs) ---
    const handleFunctionClick = useCallback(
        async (func: FunctionDto) => {
            const tabIds = onOpenTabs(func, binaryInfo);
            if (!tabIds) return;
            const { decompTabId, asmTabId } = tabIds;

            const addr = parseAddress(func.address);

            const decompPromise = (async () => {
                if (decompileCache[decompTabId] || decompileStatus[decompTabId] === "loading") {
                    return;
                }
                await fetchDecompileResult(addr, decompTabId, func.name, func.address);
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
        [binaryInfo, decompileCache, decompileStatus, asmCache, onOpenTabs, fetchDecompileResult],
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
        setDecompileStatus({});
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
        lastFidResult,
        decompileCache,
        decompileStatus,
        asmCache,
        asmHasMore,
        asmLoadingMore,
        handleOpenFile,
        handleLoadBinary,
        handleRunFid,
        handleFunctionClick,
        fetchDecompileResult,
        handleAsmLoadMore,
        handleToggleBookmark,
        handleRecordPatch,
        handleRevertPatch,
        handleClearCache,
        clearAsmCache,
    };
}
