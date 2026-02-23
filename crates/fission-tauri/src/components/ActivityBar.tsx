import type { ActivityView } from "../types";

interface ActivityBarProps {
    activeView: ActivityView;
    onViewChange: (view: ActivityView) => void;
}

const TOP_VIEWS: { id: ActivityView; icon: string; label: string }[] = [
    { id: "explorer", icon: "📁", label: "Explorer" },
    { id: "search",   icon: "🔍", label: "Search"   },
    { id: "debug",    icon: "🐛", label: "Debug"     },
    { id: "plugins",  icon: "🧩", label: "Plugins"   },
];

const BOTTOM_VIEWS: { id: ActivityView; icon: string; label: string }[] = [
    { id: "settings", icon: "⚙️", label: "Settings" },
];

function IconBtn({
    view,
    activeView,
    onViewChange,
}: {
    view: { id: ActivityView; icon: string; label: string };
    activeView: ActivityView;
    onViewChange: (v: ActivityView) => void;
}) {
    const isActive = activeView === view.id;
    return (
        <div
            className={`activity-bar__icon${isActive ? " activity-bar__icon--active" : ""}`}
            onClick={() => onViewChange(view.id)}
            title={view.label}
            role="button"
            tabIndex={0}
            onKeyDown={(e) => e.key === "Enter" && onViewChange(view.id)}
        >
            {view.icon}
        </div>
    );
}

export default function ActivityBar({ activeView, onViewChange }: ActivityBarProps) {
    return (
        <div className="activity-bar">
            <div className="activity-bar__top">
                {TOP_VIEWS.map((v) => (
                    <IconBtn key={v.id} view={v} activeView={activeView} onViewChange={onViewChange} />
                ))}
            </div>
            <div className="activity-bar__bottom">
                {BOTTOM_VIEWS.map((v) => (
                    <IconBtn key={v.id} view={v} activeView={activeView} onViewChange={onViewChange} />
                ))}
            </div>
        </div>
    );
}
