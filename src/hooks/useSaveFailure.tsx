// Parlato: one small pattern for "the backend refused this setting".
// `fail` logs the error and shows the inline note for a few seconds;
// the caller puts the control back (see lib/persist.ts, which takes `fail`
// as its last argument). Put `notice` near the control.

import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

export function useSaveFailure(showMs = 6000) {
  const { t } = useTranslation();
  const [failed, setFailed] = useState(false);
  const timer = useRef<number | null>(null);

  const clear = useCallback(() => {
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = null;
    setFailed(false);
  }, []);

  const fail = useCallback(
    (e?: unknown) => {
      if (e !== undefined) console.error(e);
      if (timer.current !== null) window.clearTimeout(timer.current);
      setFailed(true);
      timer.current = window.setTimeout(() => {
        timer.current = null;
        setFailed(false);
      }, showMs);
    },
    [showMs],
  );

  useEffect(
    () => () => {
      if (timer.current !== null) window.clearTimeout(timer.current);
    },
    [],
  );

  const notice = failed ? (
    <p role="alert" className="px-5 py-2 text-xs text-destructive">
      {t("common.saveFailed")}
    </p>
  ) : null;

  return { failed, fail, clear, notice };
}
