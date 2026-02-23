import { useEffect } from "react";

interface UseDragAndDropOptions {
    log: (msg: string) => void;
    onLoadBinary: (path: string) => Promise<void>;
}

export function useDragAndDrop({ log, onLoadBinary }: UseDragAndDropOptions) {
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
                // Tauri exposes the native path via the non-standard `.path` property
                const path = (files[0] as unknown as { path?: string }).path;
                if (path) {
                    log(`Loading (dropped): ${path}`);
                    await onLoadBinary(path);
                }
            }
        };

        document.addEventListener("dragover", handleDragOver);
        document.addEventListener("drop", handleDrop);
        return () => {
            document.removeEventListener("dragover", handleDragOver);
            document.removeEventListener("drop", handleDrop);
        };
    }, [log, onLoadBinary]);
}
