// Modal qui capture une combinaison de touches libre pour creer un
// trigger Custom (ex Ctrl+Alt+R, Win+Shift+Space, F13).
//
// Implementation : un dialog plein ecran avec un keydown listener au
// niveau document. On capture la premiere combinaison qui inclut au
// moins un modifier ET une touche finale non-modifier. ESC annule.
//
// Reference VoiceInk : KeyboardShortcuts.Recorder fait exactement la
// meme chose dans Settings. On ne capture pas les touches sticky
// (CapsLock, NumLock) ni les media keys.

import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { X } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { HotkeyTrigger } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { isMac } from "@/lib/platform";
import i18n from "@/i18n";

// VK codes des modifiers. On inclut a la fois les codes "generiques" Windows
// (0x10/0x11/0x12) que WebView2 renvoie via e.keyCode, et les variantes L/R
// (0xa0..0xa5) au cas ou un browser les emettrait. Sans 0x10/0x11/0x12, le
// recorder enregistrait "Ctrl + Ctrl" / "Shift + Shift" / "Alt + Alt" car
// la touche modifier elle-meme passait pour une touche finale.
const MODIFIER_VKS = new Set([
  0x10, 0xa0, 0xa1, // Shift (generic + L/R)
  0x11, 0xa2, 0xa3, // Ctrl (generic + L/R)
  0x12, 0xa4, 0xa5, // Alt (generic + L/R)
  0x5b, 0x5c, // Win L/R
  0x14, // CapsLock
  0x90, 0x91, // NumLock, ScrollLock
  // Parlato: WebKit on macOS reports Right Command as 93 (0x5d), which is
  // the Apps key on Windows, so only treat it as a modifier on the Mac.
  ...(isMac ? [0x5d] : []),
]);

// Touches autorisees comme combo "sans modifier". Sans cette whitelist,
// l'utilisateur pourrait enregistrer "F" seul, qui se declencherait alors
// chaque fois qu'il tape F dans n'importe quel champ texte. On limite aux
// touches non-textuelles : F1-F24, Pause, PrintScreen, Insert, Delete,
// Home, End, PageUp/Down.
const STANDALONE_VKS = new Set<number>([
  0x13, // Pause
  0x21, 0x22, // PageUp, PageDown
  0x23, 0x24, // End, Home
  0x2c, 0x2d, 0x2e, // PrintScreen, Insert, Delete
  // F1..F24
  0x70, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0x77,
  0x78, 0x79, 0x7a, 0x7b, 0x7c, 0x7d, 0x7e, 0x7f,
  0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87,
]);

// Parlato: on macOS the hotkey listener matches physical key positions (US
// layout), while WebKit's e.keyCode follows the typed character (AZERTY,
// QWERTZ...) and turns Option+E/U/I/N into dead keys. So on the Mac the VK
// comes from e.code, the physical key, to match what the listener sees.
const MAC_CODE_TO_VK: Record<string, number> = {
  Space: 0x20, Enter: 0x0d, NumpadEnter: 0x0d, Tab: 0x09, Backspace: 0x08,
  Delete: 0x2e, Escape: 0x1b, Help: 0x2d, Insert: 0x2d, Home: 0x24, End: 0x23,
  PageUp: 0x21, PageDown: 0x22, ArrowLeft: 0x25, ArrowUp: 0x26,
  ArrowRight: 0x27, ArrowDown: 0x28, Minus: 0xbd, Equal: 0xbb,
  BracketLeft: 0xdb, BracketRight: 0xdd, Backslash: 0xdc, Semicolon: 0xba,
  Quote: 0xde, Comma: 0xbc, Period: 0xbe, Slash: 0xbf, Backquote: 0xc0,
  IntlBackslash: 0xe2, NumpadMultiply: 0x6a, NumpadAdd: 0x6b,
  NumpadSubtract: 0x6d, NumpadDecimal: 0x6e, NumpadDivide: 0x6f,
  NumLock: 0x0c,
};

function macVkFromCode(code: string): number | null {
  if (/^Key[A-Z]$/.test(code)) return code.charCodeAt(3);
  if (/^Digit[0-9]$/.test(code)) return code.charCodeAt(5);
  if (/^Numpad[0-9]$/.test(code)) return 0x60 + Number(code.slice(6));
  const f = /^F([0-9]{1,2})$/.exec(code);
  if (f && Number(f[1]) >= 1 && Number(f[1]) <= 20) return 0x6f + Number(f[1]);
  return MAC_CODE_TO_VK[code] ?? null;
}

const MAC_MODIFIER_CODE = /^(Shift|Control|Alt|Meta|OS|CapsLock|Fn)/;

type Captured = {
  vk: number;
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
  win: boolean;
  label: string;
};

export function HotkeyRecorder({
  open,
  onCancel,
  onCapture,
}: {
  open: boolean;
  onCancel: () => void;
  onCapture: (trigger: Extract<HotkeyTrigger, { kind: "combo" }>) => void;
}) {
  const { t } = useTranslation();
  const [captured, setCaptured] = useState<Captured | null>(null);
  const [error, setError] = useState<string | null>(null);
  const finishedRef = useRef(false);

  // Reset l'etat a chaque ouverture.
  useEffect(() => {
    if (open) {
      setCaptured(null);
      setError(null);
      finishedRef.current = false;
    }
  }, [open]);

  useEffect(() => {
    if (!open) return;

    function onKey(e: KeyboardEvent) {
      e.preventDefault();
      e.stopPropagation();

      if (e.key === "Escape" && !e.ctrlKey && !e.altKey && !e.shiftKey) {
        onCancel();
        return;
      }

      // VK code lu via DOM event keyCode (deprecated mais fiable pour
      // les touches non-imprimables sur Windows). Pour les touches
      // alphanumeriques, e.key.toUpperCase().charCodeAt(0) suffit.
      if (isMac && MAC_MODIFIER_CODE.test(e.code)) return;
      const vk = isMac
        ? macVkFromCode(e.code)
        : e.keyCode || (e.key.length === 1 ? e.key.toUpperCase().charCodeAt(0) : 0);
      if (!vk) return;

      // Si le user appuie uniquement sur un modifier, on ne valide pas.
      // Il faut une touche finale (lettre, chiffre, F-key, espace, etc.).
      if (MODIFIER_VKS.has(vk)) return;

      if (finishedRef.current) return;

      const hasModifier = e.ctrlKey || e.altKey || e.shiftKey || e.metaKey;
      // Refuse une touche textuelle (lettre/chiffre/espace) sans modifier :
      // sinon le recorder se declenche a chaque frappe normale dans tout
      // editeur. Les F-keys et touches systeme (Insert, Delete, ...) sont
      // OK seules car elles ne produisent pas de texte.
      if (!hasModifier && !STANDALONE_VKS.has(vk)) {
        setError(t("hotkey.record.needsModifier"));
        return;
      }

      // Parlato: Command shortcuts without Control or Option belong to macOS
      // and apps (Cmd+Q, Cmd+V, Cmd+Space...); the listener would steal
      // them in every app.
      if (isMac && e.metaKey && !e.ctrlKey && !e.altKey) {
        setError(t("hotkey.record.macReserved"));
        return;
      }

      setError(null);
      finishedRef.current = true;

      const trigger: Extract<HotkeyTrigger, { kind: "combo" }> = {
        kind: "combo",
        vk,
        ctrl: e.ctrlKey,
        alt: e.altKey,
        shift: e.shiftKey,
        win: e.metaKey,
      };

      const label = formatCombo(trigger);
      setCaptured({ ...trigger, label });

      // On laisse 600ms a l'utilisateur pour voir ce qu'il a capture
      // avant de fermer + commit.
      window.setTimeout(() => onCapture(trigger), 600);
    }

    document.addEventListener("keydown", onKey, { capture: true });
    return () => {
      document.removeEventListener("keydown", onKey, { capture: true });
    };
  }, [open, onCancel, onCapture, t]);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm"
      onClick={onCancel}
    >
      <div
        className="relative w-full max-w-sm rounded-lg border bg-background p-6 shadow-lg"
        onClick={(e) => e.stopPropagation()}
      >
        <button
          type="button"
          onClick={onCancel}
          className="absolute right-3 top-3 text-muted-foreground hover:text-foreground"
          aria-label={t("common.close")}
        >
          <X className="h-4 w-4" />
        </button>

        <h2 className="text-base font-semibold">
          {t("hotkey.record.title")}
        </h2>
        <p className="mt-1 text-xs text-muted-foreground">
          {t("hotkey.record.hint")}
        </p>

        <div
          className={cn(
            "mt-6 flex h-16 items-center justify-center rounded-md border-2 border-dashed bg-muted/30",
            error && "border-destructive/60 bg-destructive/5",
          )}
        >
          {captured ? (
            <span className="font-mono text-base">{captured.label}</span>
          ) : error ? (
            <span className="px-3 text-center text-xs text-destructive">{error}</span>
          ) : (
            <span className="text-sm text-muted-foreground">
              {t("hotkey.record.waiting")}
            </span>
          )}
        </div>

        <div className="mt-4 flex justify-end gap-2">
          <Button variant="ghost" size="sm" onClick={onCancel}>
            {t("common.cancel")}
          </Button>
        </div>
      </div>
    </div>
  );
}

// Parlato: keys whose name depends on the language (and on a Mac, see
// `macOverrides.keys` in the locale files). Letters, digits and F-keys are
// the same everywhere.
const VK_KEYS: Record<number, string> = {
  0x08: "backspace",
  0x09: "tab",
  0x0d: "enter",
  0x10: "shift",
  0x11: "ctrl",
  0x12: "alt",
  0x14: "capsLock",
  0x1b: "esc",
  0x20: "space",
  0x21: "pageUp",
  0x22: "pageDown",
  0x23: "end",
  0x24: "home",
  0x25: "left",
  0x26: "up",
  0x27: "right",
  0x28: "down",
  0x2c: "printScreen",
  0x2d: "insert",
  0x2e: "delete",
};

// Parlato: Mac-only keys with no Windows equivalent.
const MAC_VK_NAMES: Record<number, string> = {
  0x2d: "Help",
  0x0c: "Clear",
};

export function vkLabel(vk: number): string {
  if (isMac && MAC_VK_NAMES[vk]) return MAC_VK_NAMES[vk];
  if (VK_KEYS[vk]) return i18n.t(`keys.${VK_KEYS[vk]}`);
  if (vk >= 0x70 && vk <= 0x87) return `F${vk - 0x6f}`; // F1..F24
  if (vk >= 0x30 && vk <= 0x39) return String.fromCharCode(vk); // 0..9
  if (vk >= 0x41 && vk <= 0x5a) return String.fromCharCode(vk); // A..Z
  return `VK_${vk.toString(16).toUpperCase().padStart(2, "0")}`;
}

export function formatCombo(
  combo: Extract<HotkeyTrigger, { kind: "combo" }>,
): string {
  const parts: string[] = [];
  if (combo.ctrl) parts.push(i18n.t("keys.ctrl"));
  if (combo.alt) parts.push(i18n.t("keys.alt"));
  if (combo.shift) parts.push(i18n.t("keys.shift"));
  if (combo.win) parts.push(i18n.t("keys.win"));
  parts.push(vkLabel(combo.vk));
  return parts.join(" + ");
}
