import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ExportDto } from "../types";

interface ExportsPanelProps {
    binaryLoaded: boolean;
    onExportClick?: (address: string) => void;
}

export default function ExportsPanel({ binaryLoaded, onExportClick }: ExportsPanelProps) {
    const [exports, setExports] = useState<ExportDto[]>([]);
    const [loading, setLoading] = useState(false);

    useEffect(() => {
        if (!binaryLoaded) { setExports([]); return; }
        setLoading(true);
        invoke<ExportDto[]>("get_exports")
            .then(setExports)
            .catch(() => setExports([]))
            .finally(() => setLoading(false));
    }, [binaryLoaded]);

    if (!binaryLoaded) {
        return <div className="panel-empty">No binary loaded.</div>;
    }
    if (loading) {
        return <div className="panel-empty">Loading exports…</div>;
    }
    if (exports.length === 0) {
        return <div className="panel-empty">No exports found in this binary.</div>;
    }

    return (
        <div className="imports-table-wrap">
            <table className="data-table">
                <thead>
                    <tr>
                        <th>Address</th>
                        <th>Ordinal</th>
                        <th>Name</th>
                        <th>Forwarder</th>
                    </tr>
                </thead>
                <tbody>
                    {exports.map((exp, i) => (
                        <tr
                            key={i}
                            className="data-table__row"
                            onClick={() => onExportClick?.(exp.address)}
                        >
                            <td className="data-table__addr">{exp.address}</td>
                            <td className="data-table__enc">{exp.ordinal != null ? exp.ordinal : "—"}</td>
                            <td className="data-table__name">{exp.name}</td>
                            <td className="data-table__lib">{exp.forwarder ?? "—"}</td>
                        </tr>
                    ))}
                </tbody>
            </table>
        </div>
    );
}
