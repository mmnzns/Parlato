// Parlato: the settings of the selected mode, edited in place.

import type { PowerModeConfig } from "@/lib/tauri";
import type { Catalog } from "./config";
import { ModeHeader } from "./ModeHeader";
import { StartsSection } from "./StartsSection";
import { DoesSection } from "./DoesSection";

export function ModeEditor({
  config: c,
  others,
  catalog,
  onChange,
  confirming,
  onAskDelete,
  onCancelDelete,
  onDelete,
  onDuplicate,
  focusName,
  onNameFocused,
}: {
  config: PowerModeConfig;
  others: PowerModeConfig[];
  catalog: Catalog;
  onChange: (c: PowerModeConfig) => void;
  confirming: boolean;
  onAskDelete: () => void;
  onCancelDelete: () => void;
  onDelete: () => void;
  onDuplicate: () => void;
  focusName?: boolean;
  onNameFocused?: () => void;
}) {
  function set<K extends keyof PowerModeConfig>(key: K, value: PowerModeConfig[K]) {
    onChange({ ...c, [key]: value });
  }

  return (
    <>
      <ModeHeader
        config={c}
        set={set}
        confirming={confirming}
        onAskDelete={onAskDelete}
        onCancelDelete={onCancelDelete}
        onDelete={onDelete}
        onDuplicate={onDuplicate}
        focusName={focusName}
        onNameFocused={onNameFocused}
      />
      <StartsSection config={c} others={others} onChange={onChange} set={set} />
      <DoesSection config={c} catalog={catalog} onChange={onChange} set={set} />
    </>
  );
}
