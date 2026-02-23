import { useState, useRef, useEffect } from "react";

interface MenuBarProps {
    onOpenFile: () => void;
    onSaveProject: () => void;
    onLoadProject: () => void;
    onClearConsole: () => void;
    onClearCache: () => void;
    onOpenListing: () => void;
    onGotoAddress: () => void;
    onRename: () => void;
    onComment: () => void;
    binaryLoaded: boolean;
    onExit: () => void;
    onToggleBottomPanel: () => void;
    bottomPanelVisible: boolean;
    onAbout: () => void;
    // Phase extensions
    onSaveSnapshot: () => void;
    onLoadSnapshot: () => void;
    onToggleDynamicMode: () => void;
    dynamicMode: boolean;
    onToggleSidebar: () => void;
    sidebarVisible: boolean;
    onOpenAssemblyView: () => void;
    onOpenDecompileView: () => void;
    // Phase 8: Export
    onExportJson: () => void;
}

interface MenuItem {
    label: string;
    shortcut?: string;
    action?: () => void;
    disabled?: boolean;
    separator?: boolean;
}

interface Menu {
    label: string;
    items: MenuItem[];
}

export default function MenuBar({
    onOpenFile,
    onSaveProject,
    onLoadProject,
    onClearConsole,
    onClearCache,
    onOpenListing,
    onGotoAddress,
    onRename,
    onComment,
    binaryLoaded,
    onExit,
    onToggleBottomPanel,
    bottomPanelVisible,
    onAbout,
    onSaveSnapshot,
    onLoadSnapshot,
    onToggleDynamicMode,
    dynamicMode,
    onToggleSidebar,
    sidebarVisible,
    onOpenAssemblyView,
    onOpenDecompileView,
    onExportJson,
}: MenuBarProps) {
    const [openMenu, setOpenMenu] = useState<string | null>(null);
    const menuRef = useRef<HTMLDivElement>(null);

    const menus: Menu[] = [
        {
            label: "File",
            items: [
                { label: "Open Binary...", shortcut: "Ctrl+O", action: onOpenFile },
                { separator: true, label: "" },
                { label: "Save Project", shortcut: "Ctrl+S", action: onSaveProject, disabled: !binaryLoaded },
                { label: "Load Project...", shortcut: "Ctrl+Shift+O", action: onLoadProject },
                { separator: true, label: "" },
                { label: "Save Snapshot...", action: onSaveSnapshot },
                { label: "Load Snapshot...", action: onLoadSnapshot },
                { separator: true, label: "" },
                { label: "Export Analysis JSON...", action: onExportJson, disabled: !binaryLoaded },
                { separator: true, label: "" },
                { label: "Clear Console", action: onClearConsole },
                { label: "Clear Decompile Cache", action: onClearCache, disabled: !binaryLoaded },
                { separator: true, label: "" },
                { label: "Exit", shortcut: "Alt+F4", action: onExit },
            ],
        },
        {
            label: "Edit",
            items: [
                { label: "Go to Address...", shortcut: "G", action: onGotoAddress, disabled: !binaryLoaded },
                { label: "Rename Symbol", shortcut: "N", action: onRename, disabled: !binaryLoaded },
                { label: "Add Comment", shortcut: ";", action: onComment, disabled: !binaryLoaded },
            ],
        },
        {
            label: "Debug",
            items: [
                {
                    label: dynamicMode
                        ? "✓ Dynamic Mode  (switch to Static)"
                        : "Switch to Dynamic Mode",
                    shortcut: "F5",
                    action: onToggleDynamicMode,
                },
            ],
        },
        {
            label: "View",
            items: [
                { label: "Assembly View", action: onOpenAssemblyView, disabled: !binaryLoaded },
                { label: "Decompile View", action: onOpenDecompileView, disabled: !binaryLoaded },
                { label: "Listing View", action: onOpenListing, disabled: !binaryLoaded },
                { separator: true, label: "" },
                {
                    label: `${sidebarVisible ? "✓ " : ""}Toggle Side Bar`,
                    shortcut: "Ctrl+B",
                    action: onToggleSidebar,
                },
                {
                    label: `${bottomPanelVisible ? "✓ " : ""}Toggle Bottom Panel`,
                    shortcut: "Ctrl+J",
                    action: onToggleBottomPanel,
                },
            ],
        },
        {
            label: "Help",
            items: [
                { label: "About Fission", action: onAbout },
            ],
        },
    ];

    // Close menu on outside click
    useEffect(() => {
        const handleClick = (e: MouseEvent) => {
            if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
                setOpenMenu(null);
            }
        };
        document.addEventListener("mousedown", handleClick);
        return () => document.removeEventListener("mousedown", handleClick);
    }, []);

    return (
        <div className="menubar" ref={menuRef}>
            <div className="menubar__brand">⚛ Fission</div>
            {menus.map((menu) => (
                <div
                    key={menu.label}
                    className={`menubar__item ${openMenu === menu.label ? "menubar__item--active" : ""}`}
                    onMouseDown={() => setOpenMenu(openMenu === menu.label ? null : menu.label)}
                    onMouseEnter={() => openMenu && setOpenMenu(menu.label)}
                >
                    {menu.label}
                    {openMenu === menu.label && (
                        <div className="menubar__dropdown">
                            {menu.items.map((item, idx) =>
                                item.separator ? (
                                    <div key={idx} className="menubar__separator" />
                                ) : (
                                    <div
                                        key={item.label}
                                        className={`menubar__dropdown-item ${item.disabled ? "menubar__dropdown-item--disabled" : ""}`}
                                        onClick={(e) => {
                                            e.stopPropagation();
                                            if (!item.disabled && item.action) {
                                                item.action();
                                                setOpenMenu(null);
                                            }
                                        }}
                                    >
                                        <span>{item.label}</span>
                                        {item.shortcut && (
                                            <span className="menubar__shortcut">{item.shortcut}</span>
                                        )}
                                    </div>
                                ),
                            )}
                        </div>
                    )}
                </div>
            ))}
        </div>
    );
}

