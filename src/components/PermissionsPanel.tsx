// Page Permissions.
//
// Reference VoiceInk Views/PermissionsView.swift : VStack de PermissionCards
// (icone + titre + description + status dot + bouton action + InfoTip).
//
// Sur Windows les permissions concernees sont : microphone, OCR
// (Windows.Media.Ocr), auto-demarrage, et le hook clavier global (toujours
// ok sous Win32).

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  CircleAlert,
  CircleCheck,
  ExternalLink,
  Keyboard,
  Languages,
  Loader2,
  Mic,
  Power,
  RefreshCw,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Section } from "@/components/ui/section";
import { InfoTip } from "@/components/ui/info-tip";
import { api, type PermissionState, type PermissionStatus } from "@/lib/tauri";
import { cn } from "@/lib/utils";

export function PermissionsPanel() {
  const { t } = useTranslation();
  const [status, setStatus] = useState<PermissionStatus | null>(null);
  const [loading, setLoading] = useState(false);

  async function refresh() {
    setLoading(true);
    try {
      const s = await api.checkPermissions();
      setStatus(s);
    } catch (e) {
      console.error(e);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    refresh();
  }, []);

  async function toggleAutostart(enabled: boolean) {
    try {
      await api.setAutostartEnabled(enabled);
      await refresh();
    } catch (e) {
      console.error(e);
    }
  }

  return (
    <Section
      title={t("settings.sectionPermissions")}
      description={t("permissions.intro")}
      action={
        <Button size="sm" variant="outline" onClick={refresh} disabled={loading}>
          {loading ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <RefreshCw className="h-3.5 w-3.5" />
          )}
          {t("permissions.refresh")}
        </Button>
      }
    >
      <PermissionRow
        icon={Mic}
        title={t("permissions.microphoneTitle")}
        description={t("permissions.microphoneDescription")}
        state={status?.microphone}
        action={
          <Button size="sm" variant="outline" onClick={() => api.openPrivacyMicrophone()}>
            {t("permissions.microphoneAction")}
            <ExternalLink className="h-3.5 w-3.5" />
          </Button>
        }
        tip={<InfoTip>{t("permissions.microphoneTip")}</InfoTip>}
      />
      <PermissionRow
        icon={Languages}
        title={t("permissions.ocrTitle")}
        description={t("permissions.ocrDescription")}
        state={status?.ocr}
        action={
          <Button size="sm" variant="outline" onClick={() => api.openLanguageSettings()}>
            {t("permissions.ocrAction")}
            <ExternalLink className="h-3.5 w-3.5" />
          </Button>
        }
        tip={<InfoTip>{t("permissions.ocrTip")}</InfoTip>}
      />
      <PermissionRow
        icon={Power}
        title={t("permissions.autostartTitle")}
        description={t("permissions.autostartDescription")}
        state={status?.autostart}
        action={
          status?.autostart ? (
            <Button
              size="sm"
              variant={status.autostart.ok ? "outline" : "default"}
              onClick={() => toggleAutostart(!status.autostart.ok)}
            >
              {status.autostart.ok
                ? t("permissions.autostartDeactivate")
                : t("permissions.autostartActivate")}
            </Button>
          ) : null
        }
      />
      <PermissionRow
        icon={Keyboard}
        title={t("permissions.hotkeyTitle")}
        description={t("permissions.hotkeyDescription")}
        state={status?.hotkey}
        tip={<InfoTip>{t("permissions.hotkeyTip")}</InfoTip>}
      />
    </Section>
  );
}

function PermissionRow({
  icon: Icon,
  title,
  description,
  state,
  action,
  tip,
}: {
  icon: LucideIcon;
  title: string;
  description: string;
  state?: PermissionState;
  action?: React.ReactNode;
  tip?: React.ReactNode;
}) {
  const { t } = useTranslation();
  const ok = state?.ok ?? false;
  return (
    <div className="flex items-center gap-5 px-5 py-[13px]">
      <Icon className="h-4 w-4 flex-none text-muted-foreground" />
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <div className="flex items-center gap-1.5">
          <span className="font-medium">{title}</span>
          {tip}
        </div>
        <span className="text-[13px] leading-[18px] text-pretty text-muted-foreground">{description}</span>
        {state?.hint_key && !state.ok && (
          <span className="text-xs text-muted-foreground">{t(state.hint_key)}</span>
        )}
        {state?.diagnostic && !state.ok && (
          <span className="font-mono text-[10px] break-all text-muted-foreground/70">{state.diagnostic}</span>
        )}
      </div>
      <div className="flex flex-none items-center gap-3">
        {state && (
          <span
            className={cn(
              "flex items-center gap-1.5 text-[13px] font-semibold whitespace-nowrap",
              ok ? "text-positive" : "text-warn-foreground",
            )}
          >
            {ok ? <CircleCheck className="h-[15px] w-[15px]" /> : <CircleAlert className="h-[15px] w-[15px]" />}
            {t(state.label_key, state.label_args ?? undefined)}
          </span>
        )}
        {action}
      </div>
    </div>
  );
}
