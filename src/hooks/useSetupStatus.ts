// Parlato: live "can I dictate?" status (see lib/setupStatus.ts). One shared
// store feeds the sidebar, the Home header and the setup wizard, so they can
// never disagree: a single load runs at a time and every reader gets its
// result. It re-reads on navigation (`refreshKey`), when the window gets
// focus again (after System Settings or a model downloaded elsewhere), when
// the app language changes (it changes the model name), and on the events
// that say the model or the shortcut changed.

import { useEffect, useSyncExternalStore } from "react";
import { subscribeSetup, getSetupSnapshot, refreshSetupStatus } from "@/lib/setupStore";
import type { SetupStatus } from "@/lib/setupStatus";

export function useSetupStatus(refreshKey?: unknown): SetupStatus | null {
  const status = useSyncExternalStore(subscribeSetup, getSetupSnapshot);
  useEffect(() => {
    refreshSetupStatus();
  }, [refreshKey]);
  return status;
}
