import type { ActivityView } from "../types";

interface ActivityBarProps {
    activeView: ActivityView;
    onViewChange: (view: ActivityView) => void;
}

const VIEWS: { id: ActivityView; icon: string; label: string }[] = [
    { id: "explorer", icon: "📁", label: "Explorer" },
    { id: "search", icon: "🔍", label: "Search" },
    { id: "debug", icon: "🐛", label: "Debug" },
    { id: "settings", icon: "⚙️", label: "Settings" },
];

export default function ActivityBar({ activeView, onViewChange }: ActivityBarProps) {
    return (
        <div className="activity-bar">
            {VIEWS.map((v) => (
                <div
                    key={v.id}
                    className={`activity-bar__icon ${activeView === v.id ? "activity-bar__icon--active" : ""}`}
                    onClick={() => onViewChange(v.id)}
                    title={v.label}
                >
                    {v.icon}
                </div>
            ))}
        </div>
    );
}
