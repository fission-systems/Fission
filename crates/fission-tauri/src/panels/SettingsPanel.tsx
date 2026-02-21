import { useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppSettings } from "../types";

interface SettingsPanelProps {
    settings: AppSettings;
    onSettingsChange: (settings: AppSettings) => void;
    onLog: (msg: string) => void;
    onClearCache?: () => void;
}

export default function SettingsPanel({ settings, onSettingsChange, onLog, onClearCache }: SettingsPanelProps) {
    const update = useCallback(
        (partial: Partial<AppSettings>) => {
            const next = { ...settings, ...partial };
            onSettingsChange(next);
            invoke("save_settings", { settings: next }).catch((e) =>
                onLog(`Settings save error: ${e}`)
            );
        },
        [settings, onSettingsChange, onLog]
    );

    const handleClearCache = useCallback(async () => {
        try {
            onClearCache?.();
            await invoke("clear_decompiler_cache");
            onLog("Decompile cache cleared.");
        } catch (e) {
            onLog(`Clear cache error: ${e}`);
        }
    }, [onLog, onClearCache]);

    return (
        <div className="settings-panel">
            <div className="settings-panel__section">
                <div className="settings-panel__section-title">Appearance</div>

                <div className="settings-panel__row">
                    <label className="settings-panel__label">Theme</label>
                    <select
                        className="settings-panel__select"
                        value={settings.theme}
                        onChange={(e) =>
                            update({ theme: e.target.value as AppSettings["theme"] })
                        }
                    >
                        <option value="dark">Dark</option>
                        <option value="light">Light</option>
                        <option value="system">System</option>
                    </select>
                </div>

                <div className="settings-panel__row">
                    <label className="settings-panel__label">
                        Font Size&nbsp;<span className="settings-panel__value">{settings.font_size}px</span>
                    </label>
                    <input
                        type="range"
                        className="settings-panel__slider"
                        min={10}
                        max={24}
                        step={1}
                        value={settings.font_size}
                        onChange={(e) => update({ font_size: Number(e.target.value) })}
                    />
                </div>
            </div>

            <div className="settings-panel__section">
                <div className="settings-panel__section-title">Decompiler</div>

                <div className="settings-panel__row">
                    <label className="settings-panel__label">Output Style</label>
                    <select
                        className="settings-panel__select"
                        value={settings.decompile_style}
                        onChange={(e) =>
                            update({
                                decompile_style: e.target.value as AppSettings["decompile_style"],
                            })
                        }
                    >
                        <option value="c-like">C-like</option>
                        <option value="pseudo">Pseudocode</option>
                        <option value="verbose">Verbose</option>
                    </select>
                </div>

                <div className="settings-panel__row">
                    <label className="settings-panel__label">
                        Simplify Level&nbsp;
                        <span className="settings-panel__value">{settings.simplify_level}</span>
                    </label>
                    <input
                        type="range"
                        className="settings-panel__slider"
                        min={0}
                        max={3}
                        step={1}
                        value={settings.simplify_level}
                        onChange={(e) => update({ simplify_level: Number(e.target.value) })}
                    />
                    <div className="settings-panel__hint">
                        0 = off · 1 = light · 2 = moderate · 3 = aggressive
                    </div>
                </div>
            </div>

            <div className="settings-panel__section">
                <div className="settings-panel__section-title">Maintenance</div>
                <div className="settings-panel__row">
                    <button
                        className="settings-panel__btn settings-panel__btn--danger"
                        onClick={handleClearCache}
                        title="Force re-decompilation of all functions"
                    >
                        Clear Decompile Cache
                    </button>
                </div>
            </div>
        </div>
    );
}
