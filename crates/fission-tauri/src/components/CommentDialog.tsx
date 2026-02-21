import { useState, useEffect, useRef } from "react";

interface CommentDialogProps {
    open: boolean;
    address: string;
    currentComment: string;
    onClose: () => void;
    onSave: (address: string, comment: string) => void;
}

export default function CommentDialog({ open, address, currentComment, onClose, onSave }: CommentDialogProps) {
    const [value, setValue] = useState("");
    const inputRef = useRef<HTMLInputElement>(null);

    useEffect(() => {
        if (open) {
            setValue(currentComment);
            setTimeout(() => {
                inputRef.current?.focus();
                inputRef.current?.select();
            }, 50);
        }
    }, [open, currentComment]);

    if (!open) return null;

    const handleSubmit = () => {
        onSave(address, value.trim());
        onClose();
    };

    return (
        <div className="dialog-overlay" onClick={onClose}>
            <div className="dialog" onClick={(e) => e.stopPropagation()}>
                <div className="dialog__title">Comment</div>
                <div className="dialog__label">Address: {address}</div>
                <input
                    ref={inputRef}
                    className="dialog__input"
                    value={value}
                    onChange={(e) => setValue(e.target.value)}
                    onKeyDown={(e) => {
                        if (e.key === "Enter") handleSubmit();
                        if (e.key === "Escape") onClose();
                    }}
                    placeholder="Enter comment (empty to remove)"
                    spellCheck={false}
                />
                <div className="dialog__actions">
                    <button className="dialog__btn dialog__btn--primary" onClick={handleSubmit}>Save</button>
                    <button className="dialog__btn" onClick={onClose}>Cancel</button>
                </div>
            </div>
        </div>
    );
}
