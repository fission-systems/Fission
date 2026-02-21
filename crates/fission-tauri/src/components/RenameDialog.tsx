import { useState, useEffect, useRef } from "react";

interface RenameDialogProps {
    open: boolean;
    currentName: string;
    address: string;
    onClose: () => void;
    onRename: (address: string, newName: string) => void;
}

export default function RenameDialog({ open, currentName, address, onClose, onRename }: RenameDialogProps) {
    const [value, setValue] = useState("");
    const inputRef = useRef<HTMLInputElement>(null);

    useEffect(() => {
        if (open) {
            setValue(currentName);
            setTimeout(() => {
                inputRef.current?.focus();
                inputRef.current?.select();
            }, 50);
        }
    }, [open, currentName]);

    if (!open) return null;

    const handleSubmit = () => {
        onRename(address, value.trim());
        onClose();
    };

    return (
        <div className="dialog-overlay" onClick={onClose}>
            <div className="dialog" onClick={(e) => e.stopPropagation()}>
                <div className="dialog__title">Rename Symbol</div>
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
                    placeholder="New name (empty to revert)"
                    spellCheck={false}
                />
                <div className="dialog__actions">
                    <button className="dialog__btn dialog__btn--primary" onClick={handleSubmit}>Rename</button>
                    <button className="dialog__btn" onClick={onClose}>Cancel</button>
                </div>
            </div>
        </div>
    );
}
