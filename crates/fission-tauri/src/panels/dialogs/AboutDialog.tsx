// Fission — About dialog.

interface AboutDialogProps {
    open: boolean;
    onClose: () => void;
}

export default function AboutDialog({ open, onClose }: AboutDialogProps) {
    if (!open) return null;

    return (
        <div className="dialog-overlay" onClick={onClose}>
            <div
                className="dialog about-dialog"
                onClick={(e) => e.stopPropagation()}
            >
                <div className="about-dialog__header">
                    <span className="about-dialog__icon">⚛</span>
                    <span className="about-dialog__title">Fission</span>
                </div>

                <div className="about-dialog__body">
                    <p className="about-dialog__tagline">
                        A modern binary analysis &amp; reverse engineering workbench.
                    </p>
                    <table className="about-dialog__table">
                        <tbody>
                            <tr>
                                <td>Version</td>
                                <td>0.1.0</td>
                            </tr>
                            <tr>
                                <td>Framework</td>
                                <td>Tauri 2 · React 19 · Rust</td>
                            </tr>
                            <tr>
                                <td>Disassembler</td>
                                <td>Fission SLEIGH runtime</td>
                            </tr>
                        </tbody>
                    </table>
                </div>

                <div className="dialog__actions">
                    <button className="dialog__btn dialog__btn--primary" onClick={onClose}>
                        Close
                    </button>
                </div>
            </div>
        </div>
    );
}
