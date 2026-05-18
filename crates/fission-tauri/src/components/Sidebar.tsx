import type { ReactNode } from "react";

interface SidebarProps {
    title: string;
    subtitle?: string;
    children: ReactNode;
}

export default function Sidebar({ title, subtitle, children }: SidebarProps) {
    return (
        <div className="sidebar">
            <div className="sidebar__header">
                <div className="sidebar__title">{title}</div>
                {subtitle && <div className="sidebar__subtitle">{subtitle}</div>}
            </div>
            <div className="sidebar__content">{children}</div>
        </div>
    );
}
