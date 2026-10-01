// Parlato: native "Are you sure?" dialog for destructive actions.
//
// Do not use window.confirm: tauri-plugin-dialog replaces it with an async
// function (calling a command the plugin no longer has), so it returns a
// Promise, which is always truthy, and `if (!confirm(...)) return` never
// stops the action. `ask` is awaited and resolves to the user's answer.

import { ask } from "@tauri-apps/plugin-dialog";
import i18n from "@/i18n";

export async function confirmDelete(message: string, okLabel?: string): Promise<boolean> {
  try {
    return await ask(message, {
      kind: "warning",
      okLabel: okLabel ?? i18n.t("common.delete"),
      cancelLabel: i18n.t("common.cancel"),
    });
  } catch (e) {
    // If the dialog cannot open, do not delete.
    console.error(e);
    return false;
  }
}
