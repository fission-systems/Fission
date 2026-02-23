import { useState, useCallback, useRef } from "react";
import type { EditorTab, FunctionDto, BinaryInfo } from "../types";

export function useEditorTabs() {
    const [tabs, setTabs] = useState<EditorTab[]>([]);
    const [activeTabId, setActiveTabId] = useState<string | null>(null);

    // Navigation history — ref for mutation, state index for reactive canGoBack/Forward
    const historyStack = useRef<string[]>([]);
    const historyIndex = useRef(-1);
    const navigatingRef = useRef(false);
    const [historyPos, setHistoryPos] = useState(-1); // mirrors historyIndex for re-render

    const canGoBack = historyPos > 0;
    const canGoForward = historyPos < historyStack.current.length - 1;

    const pushHistory = useCallback((tabId: string) => {
        if (navigatingRef.current) return;
        const stack = historyStack.current;
        const idx = historyIndex.current;
        historyStack.current = stack.slice(0, idx + 1);
        historyStack.current.push(tabId);
        historyIndex.current = historyStack.current.length - 1;
        setHistoryPos(historyIndex.current);
    }, []);

    const goBack = useCallback(() => {
        if (historyIndex.current > 0) {
            navigatingRef.current = true;
            historyIndex.current--;
            setHistoryPos(historyIndex.current);
            setActiveTabId(historyStack.current[historyIndex.current]);
            navigatingRef.current = false;
        }
    }, []);

    const goForward = useCallback(() => {
        if (historyIndex.current < historyStack.current.length - 1) {
            navigatingRef.current = true;
            historyIndex.current++;
            setHistoryPos(historyIndex.current);
            setActiveTabId(historyStack.current[historyIndex.current]);
            navigatingRef.current = false;
        }
    }, []);

    const handleTabClick = useCallback(
        (tabId: string) => {
            setActiveTabId(tabId);
            pushHistory(tabId);
        },
        [pushHistory],
    );

    const handleCloseTab = useCallback(
        (tabId: string) => {
            setTabs((prev) => {
                const remaining = prev.filter((t) => t.id !== tabId);
                setActiveTabId((cur) => {
                    if (cur === tabId) {
                        return remaining.length > 0 ? remaining[remaining.length - 1].id : null;
                    }
                    return cur;
                });
                return remaining;
            });
        },
        [],
    );

    const openFunctionTabs = useCallback(
        (func: FunctionDto, binaryInfo: BinaryInfo | null) => {
            if (!binaryInfo) return null;
            const decompTabId = `decomp-${func.address}`;
            const asmTabId = `asm-${func.address}`;
            setTabs((prev) => {
                if (prev.find((t) => t.id === decompTabId)) return prev;
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
            return { decompTabId, asmTabId };
        },
        [pushHistory],
    );

    const openListingTab = useCallback((binaryInfo: BinaryInfo | null) => {
        if (!binaryInfo) return;
        const tabId = "listing-main";
        setTabs((prev) => {
            if (prev.find((t) => t.id === tabId)) return prev;
            return [
                ...prev,
                { id: tabId, title: "Listing", type: "listing" as const, address: "0x0", functionName: "Listing" },
            ];
        });
        setActiveTabId(tabId);
    }, []);

    const openHexTab = useCallback((func: FunctionDto, binaryInfo: BinaryInfo | null) => {
        if (!binaryInfo) return;
        const tabId = `hex-${func.address}`;
        setTabs((prev) => {
            if (prev.find((t) => t.id === tabId)) return prev;
            return [
                ...prev,
                { id: tabId, title: `${func.name} [HEX]`, type: "hexview" as const, address: func.address, functionName: func.name },
            ];
        });
        setActiveTabId(tabId);
    }, []);

    const openAssemblyTab = useCallback(
        (activeTab: EditorTab | null, binaryInfo: BinaryInfo | null) => {
            if (!activeTab || !binaryInfo) return;
            const tabId = `asm-${activeTab.address}`;
            setTabs((prev) => {
                if (prev.find((t) => t.id === tabId)) return prev;
                return [
                    ...prev,
                    { id: tabId, title: `${activeTab.functionName} [ASM]`, type: "assembly" as const, address: activeTab.address, functionName: activeTab.functionName },
                ];
            });
            setActiveTabId(tabId);
        },
        [],
    );

    const openDecompileTabFromActive = useCallback(
        (activeTab: EditorTab | null, binaryInfo: BinaryInfo | null) => {
            if (!activeTab || !binaryInfo) return;
            const tabId = `decomp-${activeTab.address}`;
            setTabs((prev) => {
                if (prev.find((t) => t.id === tabId)) return prev;
                return [
                    ...prev,
                    { id: tabId, title: activeTab.functionName, type: "decompile" as const, address: activeTab.address, functionName: activeTab.functionName },
                ];
            });
            setActiveTabId(tabId);
        },
        [],
    );

    const resetTabs = useCallback(() => {
        setTabs([]);
        setActiveTabId(null);
        historyStack.current = [];
        historyIndex.current = -1;
        setHistoryPos(-1);
    }, []);

    const updateTabNames = useCallback((address: string, newName: string) => {
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
    }, []);

    return {
        tabs,
        setTabs,
        activeTabId,
        setActiveTabId,
        pushHistory,
        canGoBack,
        canGoForward,
        goBack,
        goForward,
        handleTabClick,
        handleCloseTab,
        openFunctionTabs,
        openListingTab,
        openHexTab,
        openAssemblyTab,
        openDecompileTabFromActive,
        resetTabs,
        updateTabNames,
    };
}
