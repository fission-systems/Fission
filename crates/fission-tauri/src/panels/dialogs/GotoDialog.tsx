import { useState, useEffect, useRef } from "react";

interface GotoDialogProps {
    open: boolean;
    onClose: () => void;
    onGoto: (input: string) => void;
}

export default function GotoDialog({ open, onClose, onGoto }: GotoDialogProps) {
    const [value, setValue] = useState("");
    const inputRef = useRef<HTMLInputElement>(null);

    useEffect(() => {
        if (open) {
            setValue("");
            setTimeout(() => inputRef.current?.focus(), 50);
        }
    }, [open]);

    if (!open) return null;

    const handleSubmit = () => {
        if (value.trim()) {
            onGoto(value.trim());
            onClose();
        }
    };

    return (
        <div className="dialog-overlay" onClick={onClose}>
            <div className="dialog" onClick={(e) => e.stopPropagation()}>
                <div className="dialog__title">Go to Address</div>
                <input
                    ref={inputRef}
                    className="dialog__input"
                    value={value}
                    onChange={(e) => setValue(e.target.value)}
                    onKeyDown={(e) => {
                        if (e.key === "Enter") handleSubmit();
                        if (e.key === "Escape") onClose();
                    }}
                    placeholder="Address (0x...) or function name"
                    spellCheck={false}
                />
                <div className="dialog__actions">
                    <button className="dialog__btn dialog__btn--primary" onClick={handleSubmit}>Go</button>
                    <button className="dialog__btn" onClick={onClose}>Cancel</button>
                </div>
            </div>
        </div>
    );
}
