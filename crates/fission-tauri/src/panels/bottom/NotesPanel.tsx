import { useState, useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { BookmarkDto } from "../../types";

interface NotesEntry {
    address: string;
    text: string;
    kind: "comment" | "bookmark";
}

interface NotesPanelProps {
    binaryLoaded: boolean;
    onNoteClick?: (address: string) => void;
}

export function NotesPanel({ binaryLoaded, onNoteClick }: NotesPanelProps) {
    const [notes, setNotes] = useState<NotesEntry[]>([]);
    const [loading, setLoading] = useState(false);

    const refresh = useCallback(async () => {
        if (!binaryLoaded) return;
        setLoading(true);
        try {
            const [comments, bookmarks] = await Promise.all([
                invoke<Record<string, string>>("get_comments"),
                invoke<BookmarkDto[]>("get_bookmarks"),
            ]);
            const commentEntries: NotesEntry[] = Object.entries(comments).map(
                ([addr, text]) => ({ address: addr, text, kind: "comment" })
            );
            const bookmarkEntries: NotesEntry[] = bookmarks.map((bm) => ({
                address: bm.address,
                text: bm.label + (bm.function_name ? ` (${bm.function_name})` : ""),
                kind: "bookmark",
            }));
            setNotes([...bookmarkEntries, ...commentEntries]);
        } catch {
            setNotes([]);
        } finally {
            setLoading(false);
        }
    }, [binaryLoaded]);

    // Load on first render if binary is already loaded
    useEffect(() => { refresh(); }, [refresh]);

    if (!binaryLoaded) {
        return <div className="panel-empty">No binary loaded.</div>;
    }
    if (loading) {
        return <div className="panel-empty">Loading notes…</div>;
    }

    return (
        <div className="imports-table-wrap">
            <div style={{ display: "flex", gap: 8, padding: "4px 8px", borderBottom: "1px solid var(--fission-border)" }}>
                <button className="hex-view__btn" onClick={refresh}>↻ Refresh</button>
                <span style={{ color: "var(--fission-text-muted)", fontSize: 12, alignSelf: "center" }}>
                    {notes.length} note{notes.length !== 1 ? "s" : ""}
                </span>
            </div>
            {notes.length === 0 ? (
                <div className="panel-empty">No comments or bookmarks yet.</div>
            ) : (
                <table className="data-table">
                    <thead>
                        <tr>
                            <th>Type</th>
                            <th>Address</th>
                            <th>Note</th>
                        </tr>
                    </thead>
                    <tbody>
                        {notes.map((note, i) => (
                            <tr
                                key={i}
                                className="data-table__row"
                                onClick={() => onNoteClick?.(note.address)}
                            >
                                <td className="data-table__enc">
                                    {note.kind === "bookmark" ? "📌" : "💬"}
                                </td>
                                <td className="data-table__addr">{note.address}</td>
                                <td>{note.text}</td>
                            </tr>
                        ))}
                    </tbody>
                </table>
            )}
        </div>
    );
}
