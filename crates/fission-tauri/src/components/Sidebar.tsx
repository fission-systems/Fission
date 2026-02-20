import type { ReactNode } from "react";

interface Props {
    title: string;
    children: ReactNode;
}

export default function Sidebar({ title, children }: Props) {
    return (
        <div className="sidebar">
            <div className="sidebar__header">{title}</div>
            <div className="sidebar__content">{children}</div>
        </div>
    );
}
