import { useState, useCallback } from "react";

export function useDialogs() {
    const [gotoOpen, setGotoOpen] = useState(false);
    const [renameOpen, setRenameOpen] = useState(false);
    const [renameTarget, setRenameTarget] = useState({ address: "", name: "" });
    const [commentOpen, setCommentOpen] = useState(false);
    const [commentTarget, setCommentTarget] = useState({ address: "", comment: "" });
    const [aboutOpen, setAboutOpen] = useState(false);
    const [decompilerOptionsOpen, setDecompilerOptionsOpen] = useState(false);

    const openRename = useCallback((address: string, name: string) => {
        setRenameTarget({ address, name });
        setRenameOpen(true);
    }, []);

    const openComment = useCallback((address: string, comment: string) => {
        setCommentTarget({ address, comment });
        setCommentOpen(true);
    }, []);

    return {
        gotoOpen,
        setGotoOpen,
        renameOpen,
        setRenameOpen,
        renameTarget,
        setRenameTarget,
        openRename,
        commentOpen,
        setCommentOpen,
        commentTarget,
        setCommentTarget,
        openComment,
        aboutOpen,
        setAboutOpen,
        decompilerOptionsOpen,
        setDecompilerOptionsOpen,
    };
}
