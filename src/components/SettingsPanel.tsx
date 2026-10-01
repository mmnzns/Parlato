// Preferences globales Parla (equivalent VoiceInk Settings page).
//
// Reference VoiceInk Views/Settings/SettingsView.swift : Form(.grouped)
// avec sections Shortcuts / Additional Shortcuts / Power Mode /
// Recording Feedback / Interface / Experimental / General / Privacy /
// Backup / Diagnostics. Sur Parla on regroupe le minimum vital en
// attendant un decoupage plus fin.
//
// Parlato: Workbench Settings (docs/design/v1). Sections: General, While
// you dictate, After pasting, Privacy, Permissions, About. Shortcuts moved
// to the Microphone & shortcut page; text post-processing and history
// retention moved here from their own panels.

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import { check, type Update } from "@tauri-apps/plugin-updater";
import {
  ArrowUpRight,
  CircleCheck,
  Download,
  Loader2,
  MessageSquare,
  RefreshCw,
  FolderOpen,
  Trash2,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { InfoTip } from "@/components/ui/info-tip";
import { Block, Row, Section, Segmented, Switch, selectClass } from "@/components/ui/section";
import { PermissionsPanel } from "@/components/PermissionsPanel";
import {
  LANGUAGE_LABELS,
  SUPPORTED_LANGUAGES,
  type SupportedLanguage,
} from "@/i18n";
import { api, type RetentionSettings, type TextProcessingSettings } from "@/lib/tauri";
import { getThemePref, setThemePref, type ThemePref } from "@/lib/theme";
import { cn } from "@/lib/utils";
import { isMac } from "@/lib/platform";
import { confirmDelete } from "@/lib/confirmDelete";
import { installUpdate } from "@/lib/updater";

const REPO_URL = "https://github.com/mmnzns/Parlato";
// Credit the upstream author (profile, not just the repo).
const UPSTREAM_URL = "https://github.com/LitteRabbit-37";
const SITE_URL = "https://craftconceptsdigital.com";

const DAY_MIN = 24 * 60;
const DICTATION_PRESETS = [1, 7, 30, 90]; // days
const AUDIO_PRESETS = [1, 7, 30, 90]; // days

type UpdateState =
  | "idle"
  | "checking"
  | "current"
  | "available"
  | "downloading"
  | "failed"
  | "installFailed";

export function SettingsPanel() {
  const { t, i18n } = useTranslation();
  const [recorderStyle, setRecorderStyle] = useState<"mini" | "notch">("mini");
  const [autostart, setAutostart] = useState(false);
  const [closeToTray, setCloseToTray] = useState(true);
  const [systemMute, setSystemMute] = useState(false);
  const [resumeDelay, setResumeDelay] = useState(0.2);
  const [soundFeedback, setSoundFeedback] = useState(true);
  const [showLiveTranscript, setShowLiveTranscript] = useState(true);
  const [themePref, setThemePrefState] = useState<ThemePref>(getThemePref());
  const [text, setText] = useState<TextProcessingSettings | null>(null);
  const [fillers, setFillers] = useState("");
  const [showFillers, setShowFillers] = useState(false);
  const [retention, setRetention] = useState<RetentionSettings | null>(null);
  const [version, setVersion] = useState("");
  const [update, setUpdate] = useState<UpdateState>("idle");
  const [updateVersion, setUpdateVersion] = useState("");
  const [pendingUpdate, setPendingUpdate] = useState<Update | null>(null);
  const [updatePct, setUpdatePct] = useState<number | null>(null);

  useEffect(() => {
    api
      .getRecorderStyle()
      .then((s) => setRecorderStyle(s === "notch" ? "notch" : "mini"))
      .catch(console.error);
    api
      .checkPermissions()
      .then((p) => setAutostart(p.autostart.ok))
      .catch(console.error);
    api.getCloseToTray().then(setCloseToTray).catch(console.error);
    api.getSystemMuteEnabled().then(setSystemMute).catch(console.error);
    api.getAudioResumptionDelay().then(setResumeDelay).catch(console.error);
    api.getSoundFeedbackEnabled().then(setSoundFeedback).catch(console.error);
    api.getShowLiveTranscript().then(setShowLiveTranscript).catch(console.error);
    api.getRetentionSettings().then(setRetention).catch(console.error);
    getVersion().then(setVersion).catch(console.error);
    refreshText();
    // La case "Lancer au demarrage" du menu tray modifie le meme reglage.
    const un = listen<boolean>("settings:autostart-changed", (e) => {
      setAutostart(e.payload);
    });
    return () => {
      un.then((fn) => fn());
    };
  }, []);

  async function refreshText() {
    try {
      const s = await api.getTextProcessingSettings();
      setText(s);
      setFillers(s.filler_words.join(", "));
    } catch (e) {
      console.error(e);
    }
  }

  // Optimistic toggle helper: flip the UI, persist, roll back on error.
  async function persist(next: boolean, set: (v: boolean) => void, save: (v: boolean) => Promise<unknown>) {
    set(next);
    try {
      await save(next);
    } catch (e) {
      console.error(e);
      set(!next);
    }
  }

  async function toggleText(key: keyof TextProcessingSettings) {
    if (!text) return;
    const value = !text[key];
    try {
      if (key === "text_formatting_enabled") await api.setTextFormattingEnabled(value);
      if (key === "remove_filler_words") await api.setRemoveFillerWords(value);
      if (key === "append_trailing_space") await api.setAppendTrailingSpace(value);
      if (key === "restore_clipboard_after_paste") await api.setRestoreClipboardAfterPaste(value);
      setText({ ...text, [key]: value });
    } catch (e) {
      console.error(e);
    }
  }

  async function saveFillers() {
    const words = fillers
      .split(",")
      .map((w) => w.trim())
      .filter((w) => w.length > 0);
    try {
      await api.setFillerWords(words);
      await refreshText();
    } catch (e) {
      console.error(e);
    }
  }

  async function saveResumeDelay(secs: number) {
    const clamped = Math.max(0, Math.min(10, secs));
    setResumeDelay(clamped);
    try {
      await api.setAudioResumptionDelay(clamped);
    } catch (e) {
      console.error(e);
    }
  }

  async function changeStyle(next: "mini" | "notch") {
    setRecorderStyle(next);
    try {
      await api.setRecorderStyle(next);
    } catch (e) {
      console.error(e);
    }
  }

  async function saveRetention(next: RetentionSettings) {
    setRetention(next);
    try {
      await api.setRetentionSettings(next);
    } catch (e) {
      console.error(e);
    }
  }

  async function checkForUpdates() {
    setUpdate("checking");
    try {
      const u = await check();
      if (u) {
        setUpdateVersion(u.version);
        setPendingUpdate(u);
        setUpdate("available");
      } else {
        setUpdate("current");
      }
    } catch (e) {
      console.warn("updater check:", e);
      setUpdate("failed");
    }
  }

  // Parlato : "Mettre a jour" telecharge, installe et redemarre, comme le bandeau.
  async function installNow() {
    if (!pendingUpdate) return;
    setUpdate("downloading");
    try {
      await installUpdate(pendingUpdate, setUpdatePct);
    } catch (e) {
      console.error("updater install:", e);
      setUpdate("installFailed");
    }
  }

  function changeTheme(pref: ThemePref) {
    setThemePrefState(pref);
    setThemePref(pref);
  }

  // Retention selects: map the stored minutes/days onto presets, keeping a
  // non-preset value selectable so an existing custom choice is not lost.
  const dictationValue = !retention?.transcription_cleanup
    ? "never"
    : String(retention.transcription_retention_minutes);
  const dictationOptions = [
    { value: "never", label: t("settings.retNever") },
    ...DICTATION_PRESETS.map((d) => ({
      value: String(d * DAY_MIN),
      label: t(`settings.retAfter${d}d`),
    })),
  ];
  if (retention?.transcription_cleanup && !dictationOptions.some((o) => o.value === dictationValue)) {
    dictationOptions.push({
      value: dictationValue,
      label: t("settings.retMinutes", { n: retention.transcription_retention_minutes }),
    });
  }
  const audioValue = !retention?.audio_cleanup ? "forever" : String(retention.audio_retention_days);
  const audioOptions = [
    { value: "forever", label: t("settings.audioForever") },
    ...AUDIO_PRESETS.map((d) => ({ value: String(d), label: t(`settings.audio${d}d`) })),
  ];
  if (retention?.audio_cleanup && !audioOptions.some((o) => o.value === audioValue)) {
    audioOptions.push({ value: audioValue, label: t("settings.audioDays", { n: retention.audio_retention_days }) });
  }

  const link = (label: string, url: string, Icon = ArrowUpRight) => (
    <Button size="sm" variant="outline" onClick={() => openUrl(url)}>
      {label}
      <Icon className="h-3.5 w-3.5" />
    </Button>
  );

  return (
    <>
      <Section title={t("settings.sectionGeneral")}>
        <Row label={t("settings.appLanguage")} description={t("settings.appLanguageDescription")}>
          <select
            aria-label={t("settings.appLanguage")}
            value={i18n.resolvedLanguage}
            onChange={(e) => i18n.changeLanguage(e.target.value as SupportedLanguage)}
            className={selectClass}
          >
            {SUPPORTED_LANGUAGES.map((lng) => (
              <option key={lng} value={lng}>
                {LANGUAGE_LABELS[lng]}
              </option>
            ))}
          </select>
        </Row>
        <Row label={t("settings.appearance")} description={t("settings.appearanceDescription")}>
          <Segmented
            value={themePref}
            onChange={changeTheme}
            options={[
              { value: "system", label: t("settings.appearanceSystem") },
              { value: "light", label: t("settings.appearanceLight") },
              { value: "dark", label: t("settings.appearanceDark") },
            ]}
          />
        </Row>
        <Row htmlFor="set-autostart" label={t("settings.autostartLabel")} description={t("settings.autostartHint")}>
          <Switch
            id="set-autostart"
            checked={autostart}
            onChange={(v) => persist(v, setAutostart, api.setAutostartEnabled)}
          />
        </Row>
        <Row htmlFor="set-tray" label={t("settings.trayLabel")} description={t("settings.trayHint")}>
          <Switch id="set-tray" checked={closeToTray} onChange={(v) => persist(v, setCloseToTray, api.setCloseToTray)} />
        </Row>
      </Section>

      <Section title={t("settings.sectionDictating")}>
        <Row htmlFor="set-sound" label={t("settings.soundsLabel")} description={t("settings.soundsHint")}>
          <Switch
            id="set-sound"
            checked={soundFeedback}
            onChange={(v) => persist(v, setSoundFeedback, api.setSoundFeedbackEnabled)}
          />
        </Row>
        {/* Parlato: pausing other audio is not available on Mac yet. */}
        {!isMac && (
        <Row htmlFor="set-mute" label={t("settings.muteLabel")} description={t("settings.muteHint")}>
          <Switch
            id="set-mute"
            checked={systemMute}
            onChange={(v) => persist(v, setSystemMute, api.setSystemMuteEnabled)}
          />
        </Row>
        )}
        {!isMac && systemMute && (
          <Row label={t("settings.resumeLabel")} description={t("settings.resumeHint")}>
            <input
              type="number"
              aria-label={t("settings.resumeLabel")}
              min={0}
              max={10}
              step={0.1}
              value={resumeDelay}
              onChange={(e) =>
                setResumeDelay(Number.isFinite(e.target.valueAsNumber) ? e.target.valueAsNumber : 0)
              }
              onBlur={(e) => saveResumeDelay(e.target.valueAsNumber || 0)}
              className="h-[34px] w-20 rounded-sm border-[1.5px] border-input bg-background px-3 text-sm"
            />
            <span className="text-xs text-muted-foreground">{t("common.seconds")}</span>
          </Row>
        )}
        <Row
          htmlFor="set-live"
          label={
            <span className="inline-flex items-center gap-1.5">
              {t("settings.liveLabel")}
              <InfoTip>{t("settings.liveTranscriptInfo")}</InfoTip>
            </span>
          }
          description={t("settings.liveHint")}
        >
          <Switch
            id="set-live"
            checked={showLiveTranscript}
            onChange={(v) => persist(v, setShowLiveTranscript, api.setShowLiveTranscript)}
          />
        </Row>
        <Row
          label={
            <span className="inline-flex items-center gap-1.5">
              {t("settings.recorderPosLabel")}
              <InfoTip>{t("settings.recorderStyleInfo")}</InfoTip>
            </span>
          }
          description={t("settings.recorderPosHint")}
        >
          <StyleTile
            active={recorderStyle === "mini"}
            label={t("settings.recorderStyleMini")}
            onClick={() => changeStyle("mini")}
            orientation="bottom"
          />
          <StyleTile
            active={recorderStyle === "notch"}
            label={t("settings.recorderStyleNotch")}
            onClick={() => changeStyle("notch")}
            orientation="top"
          />
        </Row>
      </Section>

      <Section title={t("settings.sectionAfterPaste")}>
        {text && (
          <>
            <Row htmlFor="set-space" label={t("settings.spaceLabel")} description={t("settings.spaceHint")}>
              <Switch
                id="set-space"
                checked={text.append_trailing_space}
                onChange={() => toggleText("append_trailing_space")}
              />
            </Row>
            <Row htmlFor="set-clip" label={t("settings.clipLabel")} description={t("settings.clipHint")}>
              <Switch
                id="set-clip"
                checked={text.restore_clipboard_after_paste}
                onChange={() => toggleText("restore_clipboard_after_paste")}
              />
            </Row>
            <Row htmlFor="set-format" label={t("settings.formatLabel")} description={t("settings.formatHint")}>
              <Switch
                id="set-format"
                checked={text.text_formatting_enabled}
                onChange={() => toggleText("text_formatting_enabled")}
              />
            </Row>
            <Row htmlFor="set-fillers" label={t("settings.fillersLabel")} description={t("settings.fillersHint")}>
              <Button size="sm" variant="ghost" onClick={() => setShowFillers((s) => !s)}>
                {showFillers ? t("settings.hideList") : t("settings.editList")}
              </Button>
              <Switch
                id="set-fillers"
                checked={text.remove_filler_words}
                onChange={() => toggleText("remove_filler_words")}
              />
            </Row>
            {showFillers && (
              <Block className="flex flex-col gap-2">
                <div className="flex gap-2">
                  <input
                    aria-label={t("postProcessing.fillersLabel")}
                    value={fillers}
                    onChange={(e) => setFillers(e.target.value)}
                    className="h-9 flex-1 rounded-sm border-[1.5px] border-input bg-background px-3 text-sm"
                  />
                  <Button size="sm" variant="outline" onClick={saveFillers}>
                    {t("postProcessing.save")}
                  </Button>
                </div>
                <p className="text-xs text-muted-foreground">
                  {t("settings.fillersListHint")} {t("postProcessing.fillersDefault")}
                </p>
              </Block>
            )}
          </>
        )}
      </Section>

      <Section title={t("settings.sectionPrivacy")}>
        {retention && (
          <>
            <Row label={t("settings.deleteDictations")} description={t("settings.deleteDictationsHint")}>
              <select
                aria-label={t("settings.deleteDictations")}
                value={dictationValue}
                onChange={(e) =>
                  saveRetention(
                    e.target.value === "never"
                      ? { ...retention, transcription_cleanup: false }
                      : {
                          ...retention,
                          transcription_cleanup: true,
                          transcription_retention_minutes: Number(e.target.value),
                        },
                  )
                }
                className={selectClass}
              >
                {dictationOptions.map((o) => (
                  <option key={o.value} value={o.value}>
                    {o.label}
                  </option>
                ))}
              </select>
            </Row>
            <Row
              label={t("settings.keepAudio")}
              description={
                retention.transcription_cleanup ? t("settings.keepAudioFollows") : t("settings.keepAudioHint")
              }
              disabled={retention.transcription_cleanup}
            >
              <select
                aria-label={t("settings.keepAudio")}
                value={audioValue}
                disabled={retention.transcription_cleanup}
                onChange={(e) =>
                  saveRetention(
                    e.target.value === "forever"
                      ? { ...retention, audio_cleanup: false }
                      : { ...retention, audio_cleanup: true, audio_retention_days: Number(e.target.value) },
                  )
                }
                className={selectClass}
              >
                {audioOptions.map((o) => (
                  <option key={o.value} value={o.value}>
                    {o.label}
                  </option>
                ))}
              </select>
            </Row>
          </>
        )}
      </Section>

      <PermissionsPanel />

      <Section title={t("settings.sectionAbout")}>
        <Row label={t("settings.versionLabel")} description={t("settings.versionHint", { version })}>
          {update !== "idle" && (
            <span
              className={cn(
                "flex items-center gap-1.5 text-[13px] font-semibold whitespace-nowrap",
                update === "current" && "text-positive",
                update === "available" && "text-foreground",
                update !== "current" && update !== "available" && "text-muted-foreground",
              )}
            >
              {(update === "checking" || update === "downloading") && (
                <Loader2 className="h-[15px] w-[15px] animate-spin" />
              )}
              {update === "current" && <CircleCheck className="h-[15px] w-[15px]" />}
              {update === "checking" && t("settings.checking")}
              {update === "current" && t("settings.upToDate")}
              {update === "available" && t("settings.updateAvailable", { version: updateVersion })}
              {update === "failed" && t("settings.checkFailed")}
              {update === "downloading" &&
                t("updater.downloading", { percent: updatePct !== null ? ` (${updatePct}%)` : "" })}
              {update === "installFailed" && t("settings.installFailed")}
            </span>
          )}
          {update === "available" || update === "downloading" || update === "installFailed" ? (
            // Parlato : une mise a jour existe, on l'installe directement.
            <Button size="sm" onClick={installNow} disabled={update === "downloading"}>
              {t("settings.updateNow")}
              <Download className="h-3.5 w-3.5" />
            </Button>
          ) : (
            <Button size="sm" variant="outline" onClick={checkForUpdates} disabled={update === "checking"}>
              {t("settings.checkUpdates")}
              <RefreshCw className="h-3.5 w-3.5" />
            </Button>
          )}
        </Row>
        <Row label={t("settings.releaseNotesLabel")} description={t("settings.releaseNotesHint")}>
          {link(t("settings.releaseNotesButton"), `${REPO_URL}/releases`)}
        </Row>
        <Row label={t("settings.feedbackLabel")} description={t("settings.feedbackHint")}>
          {link(t("settings.feedbackButton"), `${REPO_URL}/issues`, MessageSquare)}
        </Row>
        <Row label={t("settings.logsLabel")} description={t("settings.logsHint")}>
          <Button size="sm" variant="outline" onClick={() => api.openLogFolder().catch(console.error)}>
            {t("settings.logsButton")}
            <FolderOpen className="h-3.5 w-3.5" />
          </Button>
        </Row>
        {/* Parlato: removes everything Parlato stored, so uninstalling leaves nothing behind. */}
        <Row label={t("settings.deleteAllLabel")} description={t("settings.deleteAllHint")}>
          <Button
            size="sm"
            variant="destructive"
            onClick={async () => {
              const question = `${t("settings.deleteAllConfirm")}\n\n${t(isMac ? "settings.deleteAllAfterMac" : "settings.deleteAllAfterWindows")}`;
              if (!(await confirmDelete(question, t("settings.deleteAllButton")))) return;
              await api.deleteAllData().catch(console.error);
            }}
          >
            {t("settings.deleteAllButton")}
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        </Row>
        <Row label={t("settings.madeByLabel")} description={t("settings.madeByHint")}>
          {link(t("settings.madeByButton"), SITE_URL)}
        </Row>
        <Row label={t("settings.basedOnLabel")} description={t("settings.basedOnHint")}>
          {link(t("settings.basedOnButton"), UPSTREAM_URL)}
        </Row>
        <Row label={t("settings.licenceLabel")} description={t("settings.licenceHint")}>
          {link(t("settings.licenceButton"), REPO_URL)}
        </Row>
        {/* Parlato: credits and licences of the downloadable models. */}
        <Row label={t("settings.modelLicencesLabel")} description={t("settings.modelLicencesHint")}>
          {link(t("settings.modelLicencesButton"), `${REPO_URL}/blob/main/THIRD_PARTY_NOTICES.md`)}
        </Row>
      </Section>
    </>
  );
}

function StyleTile({
  active,
  label,
  orientation,
  onClick,
}: {
  active: boolean;
  label: string;
  orientation: "top" | "bottom";
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={active}
      onClick={onClick}
      className={cn(
        "flex flex-col gap-1.5 rounded-lg border-[1.5px] bg-card p-1.5 text-xs font-medium transition-colors",
        active ? "border-edge shadow-btn" : "border-border hover:bg-accent",
      )}
    >
      <span className="relative block h-14 w-[104px] rounded-sm border bg-muted">
        <span
          className={cn(
            "absolute left-1/2 h-2 w-9 -translate-x-1/2 rounded-full bg-[#141416] dark:bg-foreground",
            orientation === "top" ? "top-0" : "bottom-1.5",
          )}
        />
      </span>
      {label}
    </button>
  );
}
