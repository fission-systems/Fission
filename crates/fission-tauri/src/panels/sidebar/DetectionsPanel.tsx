import type { DetectionInfo } from "../../types";
import "../../styles/panels/sidebar/detections.css";

interface DetectionsPanelProps {
    detections: DetectionInfo[];
}

export default function DetectionsPanel({ detections }: DetectionsPanelProps) {
    if (detections.length === 0) {
        return (
            <div className="detections-panel detections-panel--empty">
                <span>No static detections</span>
            </div>
        );
    }

    return (
        <div className="detections-panel">
            <div className="detections-panel__header">
                Detections ({detections.length})
            </div>
            <div className="detections-panel__list">
                {detections.map((detection, index) => (
                    <div
                        className="detections-panel__item"
                        key={`${detection.detection_type}-${detection.name}-${index}`}
                    >
                        <div className="detections-panel__item-top">
                            <span className="detections-panel__type">
                                {detection.detection_type}
                            </span>
                            <span
                                className={`detections-panel__confidence detections-panel__confidence--${detection.confidence.toLowerCase()}`}
                            >
                                {detection.confidence}
                            </span>
                        </div>
                        <div className="detections-panel__name">
                            {detection.name}
                            {detection.version ? (
                                <span className="detections-panel__version">
                                    {" "}
                                    {detection.version}
                                </span>
                            ) : null}
                        </div>
                        {detection.details ? (
                            <div
                                className="detections-panel__details"
                                title={detection.details}
                            >
                                {detection.details}
                            </div>
                        ) : null}
                    </div>
                ))}
            </div>
        </div>
    );
}
