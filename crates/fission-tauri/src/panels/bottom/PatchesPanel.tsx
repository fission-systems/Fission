import type { PatchRecord } from "../../types";

interface PatchesPanelProps {
    patches: PatchRecord[];
    onRevert?: (rec: PatchRecord) => void;
}

export default function PatchesPanel({ patches, onRevert }: PatchesPanelProps) {
    if (patches.length === 0) {
        return (
            <div className="panel-empty">
                No patches applied yet. Use the Hex editor tab to patch bytes.
            </div>
        );
    }

    const toHex = (bytes: number[]) =>
        bytes.map((b) => b.toString(16).padStart(2, "0")).join(" ");

    return (
        <div className="imports-table-wrap">
            <table className="data-table">
                <thead>
                    <tr>
                        <th>Address</th>
                        <th>Label</th>
                        <th>Original</th>
                        <th>Patched</th>
                        <th></th>
                    </tr>
                </thead>
                <tbody>
                    {patches.map((rec, i) => (
                        <tr key={i} className="data-table__row">
                            <td className="data-table__addr">0x{rec.address.toString(16)}</td>
                            <td>{rec.label}</td>
                            <td className="data-table__enc">{toHex(rec.original)}</td>
                            <td className="data-table__enc">{toHex(rec.patched)}</td>
                            <td>
                                <button
                                    className="hex-view__btn"
                                    onClick={() => onRevert?.(rec)}
                                    title="Revert this patch"
                                >
                                    ↩ Revert
                                </button>
                            </td>
                        </tr>
                    ))}
                </tbody>
            </table>
        </div>
    );
}
