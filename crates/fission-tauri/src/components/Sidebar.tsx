import type { ReactNode } from "react";

interface SidebarProps {
    title: string;
    children: ReactNode;
}

export default function Sidebar({ title, children }: SidebarProps) {
    return (
        <div className="sidebar">
            <div className="sidebar__header">{title}</div>
            <div className="sidebar__content">{children}</div>
        </div>
    );
}
