// Fission — Sections explorer panel (sidebar widget).
// Displays the list of binary sections returned by `get_sections`.

import type { SectionDto } from "../../types";

interface SectionsPanelProps {
    sections: SectionDto[];
}

export default function SectionsPanel({ sections }: SectionsPanelProps) {
    if (sections.length === 0) {
        return (
            <div className="sections-panel sections-panel--empty">
                <span>No sections</span>
            </div>
        );
    }

    return (
        <div className="sections-panel">
            <div className="sections-panel__header">
                Sections ({sections.length})
            </div>
            <table className="sections-panel__table">
                <thead>
                    <tr>
                        <th>Name</th>
                        <th>Address</th>
                        <th>Size</th>
                        <th>Flags</th>
                    </tr>
                </thead>
                <tbody>
                    {sections.map((sec, i) => (
                        <tr key={i} className="sections-panel__row">
                            <td className="sections-panel__name">{sec.name}</td>
                            <td className="sections-panel__addr">{sec.address}</td>
                            <td className="sections-panel__size">
                                {sec.size >= 1024 * 1024
                                    ? `${(sec.size / 1048576).toFixed(1)} MiB`
                                    : sec.size >= 1024
                                    ? `${(sec.size / 1024).toFixed(1)} KiB`
                                    : `${sec.size} B`}
                            </td>
                            <td className="sections-panel__flags">{sec.flags}</td>
                        </tr>
                    ))}
                </tbody>
            </table>
        </div>
    );
}
