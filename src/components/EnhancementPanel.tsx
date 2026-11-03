// Parlato: AI cleanup page, rebuilt to the Workbench design (docs/design/v1,
// screen "ai"). The page header carries the On/Off switch; below it:
//   [01] where the AI runs (this PC vs an online service)
//   [02] the engine / service, model and account key for that choice
//   [03] writing style (prompts)
//   then the screen-context toggle.
// Provider, key and prompt logic is unchanged from upstream.

import { useTranslation } from "react-i18next";
import { Switch } from "@/components/ui/section";
import { CompactHero } from "@/components/CompactHero";
import { EnhancementScreenContext } from "@/components/EnhancementScreenContext";
import { PromptEditor } from "@/components/PromptEditor";
import { EnhancementTryBox } from "@/components/EnhancementTryBox";
import { CloudSection } from "@/components/enhancement/CloudSection";
import { LocalSection } from "@/components/enhancement/LocalSection";
import { WhereSection } from "@/components/enhancement/WhereSection";
import { useEnhancementSettings } from "@/components/enhancement/useEnhancementSettings";
import { isMac } from "@/lib/platform";

export function EnhancementPanel({ crumb }: { crumb?: string }) {
  const { t } = useTranslation();
  const s = useEnhancementSettings();
  const { enabled, where, prompts, activePromptId, toggleEnabled, chooseWhere, selectPrompt, refresh } = s;

  return (
    <>
      <CompactHero
        crumb={crumb}
        title={t("hero.enhancementTitle")}
        description={t("hero.enhancementDescription")}
        action={
          <>
            <span className="text-[13px] font-semibold">{enabled ? t("ai.on") : t("ai.off")}</span>
            <Switch checked={enabled} onChange={toggleEnabled} label={t("ai.toggleLabel")} />
          </>
        }
      />

      {/* Parlato: the page stays usable while cleanup is off, so a model and
          a key can be set up first. */}
      {!enabled && (
        <p role="status" className="mb-[22px] rounded-sm border-[1.5px] border-edge bg-muted px-4 py-3 text-[13px] text-muted-foreground">
          {t("ai.offNotice")}
        </p>
      )}
      <div className="flex flex-col gap-[22px]">
        <WhereSection where={where} chooseWhere={chooseWhere} />

        {where === "local" && <LocalSection s={s} />}

        {where === "cloud" && <CloudSection s={s} />}

        <PromptEditor
          prompts={prompts}
          activeId={activePromptId}
          onSelect={selectPrompt}
          onChange={refresh}
        />

        <EnhancementTryBox />

        {/* Parlato: reading the screen (OCR) is not available on Mac yet. */}
        {isMac ? (
          <p className="rounded-lg border-[1.5px] border-dashed bg-card px-5 py-4 text-[13px] text-muted-foreground">
            {t("mac.screenNotYet")}
          </p>
        ) : (
          <EnhancementScreenContext />
        )}
      </div>
    </>
  );
}
