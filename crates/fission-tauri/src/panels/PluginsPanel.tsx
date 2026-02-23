import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { PluginInfoDto } from "../types";

interface PluginsPanelProps {
    onLog: (msg: string) => void;
}

export default function PluginsPanel({ onLog }: PluginsPanelProps) {
    const [plugins, setPlugins] = useState<PluginInfoDto[]>([]);
    const [loading, setLoading] = useState(false);

    const refreshPlugins = useCallback(async () => {
        try {
            const list = await invoke<PluginInfoDto[]>("list_plugins");
            setPlugins(list);
        } catch (e) {
            onLog(`[Plugin] List failed: ${e}`);
        }
    }, [onLog]);

    useEffect(() => {
        refreshPlugins();
    }, [refreshPlugins]);

    const handleLoadPlugin = async () => {
        try {
            const path = await open({
                multiple: false,
                filters: [
                    { name: "Native Plugin", extensions: ["so", "dylib", "dll"] },
                ],
            });
            if (!path) return;
            setLoading(true);
            const info = await invoke<PluginInfoDto>("load_plugin", { path });
            onLog(`[Plugin] Loaded: ${info.name} v${info.version}`);
            await refreshPlugins();
        } catch (e) {
            onLog(`[Plugin] Load failed: ${e}`);
        } finally {
            setLoading(false);
        }
    };

    const handleUnload = async (id: string) => {
        try {
            await invoke("unload_plugin", { pluginId: id });
            onLog(`[Plugin] Unloaded: ${id}`);
            await refreshPlugins();
        } catch (e) {
            onLog(`[Plugin] Unload failed: ${e}`);
        }
    };

    const handleToggleEnabled = async (plugin: PluginInfoDto) => {
        try {
            if (plugin.enabled) {
                await invoke("disable_plugin", { pluginId: plugin.id });
            } else {
                await invoke("enable_plugin", { pluginId: plugin.id });
            }
            await refreshPlugins();
        } catch (e) {
            onLog(`[Plugin] Toggle failed: ${e}`);
        }
    };

    return (
        <div className="plugins-panel">
            <div className="plugins-panel__toolbar">
                <button
                    className="plugins-panel__load-btn"
                    onClick={handleLoadPlugin}
                    disabled={loading}
                    title="Load a native Rust plugin (.so / .dylib / .dll)"
                >
                    {loading ? "Loading..." : "⊕ Load Plugin..."}
                </button>
                <button
                    className="plugins-panel__refresh-btn"
                    onClick={refreshPlugins}
                    title="Refresh plugin list"
                >
                    ↻
                </button>
            </div>

            {plugins.length === 0 ? (
                <div className="plugins-panel__empty">
                    <p>No plugins loaded.</p>
                    <p className="plugins-panel__hint">
                        Load a native Rust plugin compiled as a dynamic library
                        (.so / .dylib / .dll).
                    </p>
                </div>
            ) : (
                <ul className="plugins-panel__list">
                    {plugins.map((p) => (
                        <li
                            key={p.id}
                            className={`plugins-panel__item ${!p.enabled ? "plugins-panel__item--disabled" : ""}`}
                        >
                            <div className="plugins-panel__item-header">
                                <span
                                    className="plugins-panel__item-name"
                                    title={p.description}
                                >
                                    🧩 {p.name}
                                </span>
                                <span className="plugins-panel__item-version">
                                    v{p.version}
                                </span>
                            </div>
                            <div className="plugins-panel__item-meta">
                                <span className="plugins-panel__item-id">{p.id}</span>
                                <span className="plugins-panel__item-author">
                                    by {p.author}
                                </span>
                            </div>
                            {p.description && (
                                <div className="plugins-panel__item-desc">
                                    {p.description}
                                </div>
                            )}
                            <div className="plugins-panel__item-actions">
                                <button
                                    className={`plugins-panel__toggle-btn ${p.enabled ? "plugins-panel__toggle-btn--enabled" : ""}`}
                                    onClick={() => handleToggleEnabled(p)}
                                    title={p.enabled ? "Disable plugin" : "Enable plugin"}
                                >
                                    {p.enabled ? "✓ Enabled" : "✗ Disabled"}
                                </button>
                                <button
                                    className="plugins-panel__unload-btn"
                                    onClick={() => handleUnload(p.id)}
                                    title="Unload plugin from memory"
                                >
                                    Unload
                                </button>
                            </div>
                        </li>
                    ))}
                </ul>
            )}
        </div>
    );
}
