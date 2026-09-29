// Layout principal : sidebar + detail pane.
//
// Reference VoiceInk Views/ContentView.swift NavigationSplitView :
// sidebar 210pt fixe a gauche + detail pane a droite, fenetre 950x730.

import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { Sidebar, useCrumb, type View } from "@/components/Sidebar";
import { CompactHero } from "@/components/CompactHero";
import { DashboardPanel } from "@/components/DashboardPanel";
import { DictionaryPanel } from "@/components/DictionaryPanel";
import { EnhancementPanel } from "@/components/EnhancementPanel";
import { HistoryPanel } from "@/components/HistoryPanel";
import { LlmLocalPanel } from "@/components/LlmLocalPanel";
import { ModelsPage } from "@/components/ModelsPage";
import { Onboarding } from "@/components/Onboarding";
import { PermissionsPanel } from "@/components/PermissionsPanel";
import { PostProcessingPanel } from "@/components/PostProcessingPanel";
import { PowerModePanel } from "@/components/PowerModePanel";
import { SettingsPanel } from "@/components/SettingsPanel";
import { RecorderPanel } from "@/components/RecorderPanel";
import { TranscribePanel } from "@/components/TranscribePanel";
import { UpdateChecker } from "@/components/UpdateChecker";
import { VadPanel } from "@/components/VadPanel";
import { api, type GpuInfo, type RecordingStopped } from "@/lib/tauri";
import "./App.css";

function App() {
  const { t, i18n } = useTranslation();
  const [view, setView] = useState<View>("dashboard");
  const [gpu, setGpu] = useState<GpuInfo | null>(null);
  const [selectedModelId, setSelectedModelId] = useState<string | null>(null);
  const [lastWavPath, setLastWavPath] = useState<string | null>(null);
  const [onboarded, setOnboarded] = useState<boolean | null>(null);
  const crumb = useCrumb(view);

  useEffect(() => {
    api.getGpuInfo().then(setGpu).catch(console.error);
    api
      .getSelectedWhisperModel()
      .then((id) => setSelectedModelId(id))
      .catch(console.error);
    api
      .getOnboardingCompleted()
      .then(setOnboarded)
      .catch(() => setOnboarded(true));

    const unlisten = listen<RecordingStopped>("recording:stopped", (e) => {
      setLastWavPath(e.payload.wav_path);
    });

    // Tray menu triggers (navigate to a panel, copy notification, update check).
    const unNav = listen<string>("tray:navigate", (e) => {
      const target = e.payload as View;
      if (target) setView(target);
    });
    const unNotice = listen<string>("tray:notice", (e) => {
      if (!e.payload) return;
      // Minimal UX : browser alert is fine for now, keeps us dependency-free.
      // The transcript stays in the clipboard regardless of whether this
      // notice is dismissed or not.
      console.info("[tray]", e.payload);
    });
    // Le menu tray natif est traduit cote Rust : on lui reflete la langue
    // i18next courante et chaque changement.
    const syncLanguage = (lng: string) => {
      api.setUiLanguage(lng.split("-")[0]).catch(console.error);
    };
    syncLanguage(i18n.resolvedLanguage ?? i18n.language ?? "en");
    i18n.on("languageChanged", syncLanguage);

    return () => {
      unlisten.then((fn) => fn());
      unNav.then((fn) => fn());
      unNotice.then((fn) => fn());
      i18n.off("languageChanged", syncLanguage);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (onboarded === false) {
    return <Onboarding onDone={() => setOnboarded(true)} />;
  }

  function handleSelectModel(id: string | null) {
    setSelectedModelId(id);
    api.setSelectedWhisperModel(id).catch(console.error);
  }

  return (
    <div className="flex h-screen w-screen bg-background text-foreground">
      <Sidebar current={view} onSelect={setView} />
      <main
        className="min-w-0 flex-1 overflow-auto"
        style={{ backgroundImage: "var(--dot-grid)", backgroundSize: "18px 18px" }}
      >
        <UpdateChecker />
        <div className="mx-auto flex max-w-[820px] flex-col gap-[22px] px-9 pt-7 pb-12">
          {view === "dashboard" && (
            <>
              <CompactHero
                crumb={crumb}
                title={t("hero.dashboardTitle")}
                description={t("hero.dashboardDescription")}
              />
              <DashboardPanel />
              {gpu && (
                <p className="text-center text-xs text-muted-foreground">
                  {gpu.has_nvidia
                    ? t("hero.hardwareGpu", {
                        device: gpu.device_name ?? "",
                        cuda: gpu.cuda_version ?? "?",
                      })
                    : t("hero.hardwareCpu")}
                </p>
              )}
            </>
          )}

          {view === "transcribe" && (
            <>
              <CompactHero
                crumb={crumb}
                title={t("hero.transcribeTitle")}
                description={t("hero.transcribeDescription")}
              />
              <RecorderPanel />
              <TranscribePanel
                lastWavPath={lastWavPath}
                selectedModelId={selectedModelId}
              />
            </>
          )}

          {view === "history" && (
            <>
              <CompactHero
                crumb={crumb}
                title={t("hero.historyTitle")}
                description={t("hero.historyDescription")}
              />
              <HistoryPanel />
            </>
          )}

          {view === "models" && (
            <>
              <CompactHero
                crumb={crumb}
                title={t("hero.modelsTitle")}
                description={t("hero.modelsDescription")}
              />
              <ModelsPage
                selectedModelId={selectedModelId}
                onSelectModel={handleSelectModel}
              />
            </>
          )}

          {view === "enhancement" && (
            <>
              <CompactHero
                crumb={crumb}
                title={t("hero.enhancementTitle")}
                description={t("hero.enhancementDescription")}
              />
              <EnhancementPanel />
              <LlmLocalPanel />
            </>
          )}

          {view === "powermode" && (
            <>
              <CompactHero
                crumb={crumb}
                title={t("hero.powerModeTitle")}
                description={t("hero.powerModeDescription")}
              />
              <PowerModePanel />
            </>
          )}

          {view === "permissions" && (
            <>
              <CompactHero
                crumb={crumb}
                title={t("hero.permissionsTitle")}
                description={t("hero.permissionsDescription")}
              />
              <PermissionsPanel />
            </>
          )}

          {view === "audio" && (
            <>
              <CompactHero
                crumb={crumb}
                title={t("hero.audioTitle")}
                description={t("hero.audioDescription")}
              />
              <RecorderPanel />
              <VadPanel />
              <PostProcessingPanel />
            </>
          )}

          {view === "dictionary" && (
            <>
              <CompactHero
                crumb={crumb}
                title={t("hero.dictionaryTitle")}
                description={t("hero.dictionaryDescription")}
              />
              <DictionaryPanel />
            </>
          )}

          {view === "settings" && (
            <>
              <CompactHero
                crumb={crumb}
                title={t("hero.settingsTitle")}
                description={t("hero.settingsDescription")}
              />
              <SettingsPanel />
              <PostProcessingPanel />
              {/* Parlato: Permissions moved here from its own sidebar entry. */}
              <PermissionsPanel />
            </>
          )}
        </div>
      </main>
    </div>
  );
}

export default App;
