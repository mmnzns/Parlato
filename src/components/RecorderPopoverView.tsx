// Popover du bouton Mode de la bulle, rendu dans sa propre fenetre
// ("recorder-popover", voir src-tauri/src/mini_recorder.rs).
//
// Reference VoiceInk Features/Modes/Views/ModePopover.swift : liste des
// modes actives (ModeRow : icone, nom, coche sur le mode effectif), clic =
// setActiveConfiguration. Vide : "No Modes Available". Parlato : 184 px de
// large comme la bulle, titre "mode", raccourci Alt+N a droite, sans ombre.
//
// La fenetre etant separee de la bulle, le survol est remonte a la bulle
// par l'event "recorder:popover-hover" pour reproduire la logique
// RecorderModeButton.syncPopoverVisibility (fermeture 250 ms apres avoir
// quitte le bouton ET le popover).

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { emit, listen } from "@tauri-apps/api/event";
import { Check, Settings as SettingsIcon } from "lucide-react";
import { api, type PowerModeConfig, type PowerSession } from "@/lib/tauri";
import { cn, modeBadge, powerShortcutLabel } from "@/lib/utils";

export function RecorderPopoverView() {
  const { t } = useTranslation();
  const [configs, setConfigs] = useState<PowerModeConfig[]>([]);
  const [session, setSession] = useState<PowerSession | null>(null);

  useEffect(() => {
    const load = () => {
      Promise.all([api.listPowerConfigs(), api.getActivePowerSession()])
        .then(([cfgs, s]) => {
          setConfigs(cfgs);
          setSession(s);
        })
        .catch(console.error);
    };
    load();
    const unlistens = [
      listen<PowerSession | null>("power_mode:active", (e) => setSession(e.payload)),
      // La bulle demande un rechargement a chaque ouverture (profils
      // eventuellement modifies depuis la creation de la fenetre).
      listen("recorder:popover-refresh", load),
    ];
    return () => {
      Promise.all(unlistens).then((arr) => arr.forEach((fn) => fn()));
    };
  }, []);

  const hover = (hovering: boolean) => {
    emit("recorder:popover-hover", { hovering }).catch(console.error);
  };

  async function select(c: PowerModeConfig) {
    try {
      await api.selectPowerConfig(c.id);
    } catch (e) {
      console.error(e);
    }
    hover(false);
    api.closeRecorderPopover().catch(console.error);
  }

  const enabled = configs.filter((c) => c.is_enabled);

  return (
    <div
      className="flex h-screen w-screen items-end bg-transparent"
      onMouseEnter={() => hover(true)}
      onMouseLeave={() => hover(false)}
    >
      {/* Parlato: same width as the pill (184 px), dark like the pill, flat. */}
      <div className="flex max-h-full w-full flex-col overflow-hidden rounded-lg border border-white/10 bg-[#1D1D20] p-1.5 text-xs text-white">
        <p className="px-2 pb-1.5 pt-1 font-mono text-[10px] text-white/55">
          {t("miniRecorder.selectMode")}
        </p>
        <div className="overflow-auto">
          {enabled.length === 0 ? (
            <div className="px-2 py-2 text-xs">
              <p className="mb-1 font-medium">{t("miniRecorder.noModesAvailable")}</p>
              <p className="text-white/55">{t("miniRecorder.noConfigs")}</p>
              <button
                type="button"
                onClick={async () => {
                  try {
                    await api.showMainWindow("powermode");
                  } catch (e) {
                    console.error(e);
                  }
                  hover(false);
                  api.closeRecorderPopover().catch(console.error);
                }}
                className="mt-2 inline-flex items-center gap-1.5 rounded-sm bg-white/10 px-2 py-1 text-[11px] font-medium hover:bg-white/15"
              >
                <SettingsIcon className="h-3 w-3" />
                {t("miniRecorder.openPowerMode")}
              </button>
            </div>
          ) : (
            enabled.map((c) => {
              // Parlato : raccourci choisi par l'utilisateur (aucun par defaut).
              const shortcut = powerShortcutLabel(c.shortcut_slot);
              const isActive = c.id === session?.config_id;
              return (
                <button
                  key={c.id}
                  type="button"
                  onClick={() => select(c)}
                  className={cn(
                    "flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left hover:bg-white/10",
                    isActive && "bg-white/[0.08]",
                  )}
                >
                  <span className="w-4 text-center text-sm font-semibold leading-none">
                    {modeBadge(c.emoji, c.name)}
                  </span>
                  <span className={cn("flex-1 truncate", isActive && "font-semibold")}>
                    {c.name}
                  </span>
                  {isActive && <Check className="h-3 w-3 shrink-0 text-[#B5E61D]" />}
                  {shortcut && (
                    <span className="shrink-0 font-mono text-[10px] text-white/55">
                      {shortcut}
                    </span>
                  )}
                </button>
              );
            })
          )}
        </div>
      </div>
    </div>
  );
}
