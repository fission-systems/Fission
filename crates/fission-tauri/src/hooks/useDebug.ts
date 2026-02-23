import { useState, useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

interface UseDebugOptions {
    log: (msg: string) => void;
}

export function useDebug({ log }: UseDebugOptions) {
    const [dynamicMode, setDynamicMode] = useState(false);
    const [gitBranch, setGitBranch] = useState<string>("—");

    useEffect(() => {
        invoke<string>("get_git_branch")
            .then(setGitBranch)
            .catch(() => {});
    }, []);

    const handleToggleDynamicMode = useCallback(() => {
        setDynamicMode((v) => {
            const next = !v;
            log(next ? "Switched to Dynamic (Debug) mode." : "Switched to Static Analysis mode.");
            return next;
        });
    }, [log]);

    return {
        dynamicMode,
        gitBranch,
        handleToggleDynamicMode,
    };
}
