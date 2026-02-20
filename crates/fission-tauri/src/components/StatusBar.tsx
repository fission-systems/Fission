import type { BinaryInfo } from "../types";

interface Props {
    binaryInfo: BinaryInfo | null;
    functionCount: number;
}

export default function StatusBar({ binaryInfo, functionCount }: Props) {
    return (
        <div className="status-bar">
            <div className="status-bar__section">
                🔬 Fission
            </div>
            {binaryInfo && (
                <>
                    <div className="status-bar__section">
                        {binaryInfo.format} / {binaryInfo.arch}
                    </div>
                    <div className="status-bar__section">
                        Functions: {functionCount}
                    </div>
                    <div className="status-bar__section">
                        Entry: {binaryInfo.entry_point}
                    </div>
                </>
            )}
            <div className="status-bar__spacer" />
            <div className="status-bar__section">
                Phase 1
            </div>
        </div>
    );
}
