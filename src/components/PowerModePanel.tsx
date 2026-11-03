// Parlato: Power modes page, rebuilt to the Workbench design (docs/design/v1,
// screen "profiles"): the list of modes on the left, the selected mode's
// settings on the right, edited in place.
//
// Every change is saved straight away (text fields on blur), so there is no
// Save button. A new mode is created as soon as "New power mode" is
// pressed. The PowerModeConfig shape and the backend commands are
// unchanged from upstream; "Keep my usual" maps to the null values that
// mean "leave the global setting alone".

import { useTranslation } from "react-i18next";
import { Info } from "lucide-react";
import { Row, Switch } from "@/components/ui/section";
import { ModeEditor } from "@/components/power-mode/ModeEditor";
import { ModeList } from "@/components/power-mode/ModeList";
import { usePowerModes } from "@/components/power-mode/usePowerModes";

export function PowerModePanel() {
  const { t } = useTranslation();
  const {
    configs,
    selectedId,
    setSelectedId,
    autoRestore,
    catalog,
    confirming,
    setConfirming,
    createMode,
    duplicateSelected,
    save,
    removeSelected,
    toggleAutoRestore,
  } = usePowerModes();

  const selected = configs.find((c) => c.id === selectedId) ?? null;

  return (
    <>
      <div className="grid items-start gap-5 md:grid-cols-[220px_minmax(0,1fr)]">
        <ModeList
          configs={configs}
          selectedId={selectedId}
          onSelect={(id) => {
            setSelectedId(id);
            setConfirming(false);
          }}
          onCreate={createMode}
        />

        <div className="flex min-w-0 flex-col gap-[22px]">
          {selected ? (
            <ModeEditor
              key={selected.id}
              config={selected}
              others={configs.filter((c) => c.id !== selected.id)}
              catalog={catalog}
              onChange={save}
              confirming={confirming}
              onAskDelete={() => setConfirming(true)}
              onCancelDelete={() => setConfirming(false)}
              onDelete={removeSelected}
              onDuplicate={duplicateSelected}
            />
          ) : (
            <div className="rounded-lg border-[1.5px] border-dashed border-input bg-card p-8 text-center text-sm text-muted-foreground">
              {t("pm.pickOne")}
            </div>
          )}
        </div>
      </div>

      {/* A single row, not a numbered section: it applies to all modes. */}
      <div className="rounded-lg border-[1.5px] border-edge bg-card">
        <Row label={t("pm.autoRestore")} description={t("pm.autoRestoreDesc")} htmlFor="pm-auto-restore">
          <Switch id="pm-auto-restore" checked={autoRestore} onChange={toggleAutoRestore} />
        </Row>
      </div>

      <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
        <Info className="h-3.5 w-3.5" />
        {t("pm.shortcutWins")}
      </p>
    </>
  );
}
