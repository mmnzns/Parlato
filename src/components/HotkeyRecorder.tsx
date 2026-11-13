// Modal qui capture une touche seule ou une combinaison pour creer un
// trigger Custom (ex Ctrl+Alt+R, Win+Shift+Space, F13, ou "R" seul pour la
// dictée).
//
// Implementation : un dialog plein ecran avec un keydown listener au
// niveau document. On capture la premiere touche finale non-modifier (avec
// ou sans modifier selon `rule`). ESC annule (sauf pour les raccourcis de
// dictee, 0.16 : Echap peut etre la touche, on ferme avec le bouton).
// 0.16, raccourcis de dictee : deux modifiers appuyes ensemble puis relaches
// sans touche finale (Ctrl + Win) sont captures comme "modifiers".
//
// Reference VoiceInk : KeyboardShortcuts.Recorder fait exactement la
// meme chose dans Settings. On ne capture pas les touches sticky
// (CapsLock, NumLock) ni les media keys.

import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/section";
import type { HotkeyTrigger, KeySide, KeySides } from "@/lib/tauri";
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

// Touches autorisees comme combo "sans modifier" pour les raccourcis
// utilitaires (copier, coller, reessayer, historique, annuler). Ces
// raccourcis sont avales globalement : "F" seul rendrait la lettre F
// intapable partout. On limite aux touches non-textuelles : F1-F24, Pause,
// PrintScreen, Insert, Delete, Home, End, PageUp/Down.
// Les raccourcis de dictée (primary/secondary) acceptent n'importe quelle
// touche seule (voir `typesText` pour l'avertissement).
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

// Parlato: true when pressing `vk` alone does something in the focused app,
// i.e. it types a character (letters, digits, Space, punctuation/OEM keys,
// numpad digits and operators) or edits text (Enter, Tab, Backspace). False
// for the old STANDALONE_VKS keys (F1-F24, Pause, PrintScreen, Insert,
// Delete, Home, End, PageUp/Down), arrows, the Menu key, Escape and the
// media/browser keys. The listener does not swallow the dictation key, so a
// key for which this is true keeps typing when used as the dictation shortcut.
export function typesText(vk: number): boolean {
  if (STANDALONE_VKS.has(vk)) return false;
  if (vk >= 0x25 && vk <= 0x28) return false; // arrows
  if (vk === 0x5d) return false; // Menu / Apps
  if (vk === 0x1b) return false; // Escape
  if (vk >= 0xa6 && vk <= 0xb7) return false; // browser / media / launch keys
  return true;
}

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

export type ComboTrigger = Extract<HotkeyTrigger, { kind: "combo" }>;
export type ChordTrigger = Extract<HotkeyTrigger, { kind: "modifiers" }>;

type ModKind = "ctrl" | "alt" | "shift" | "win";
const MOD_KINDS: ModKind[] = ["ctrl", "alt", "shift", "win"];

// Parlato 0.16: which modifier (and which side) a keyboard event code is.
// `e.code` tells left from right on both Windows and the Mac.
function modifierOfCode(code: string): { kind: ModKind; side: "left" | "right" } | null {
  const m = /^(Control|Alt|Shift|Meta|OS)(Left|Right)$/.exec(code);
  if (!m) return null;
  const kind: ModKind =
    m[1] === "Control" ? "ctrl" : m[1] === "Alt" ? "alt" : m[1] === "Shift" ? "shift" : "win";
  return { kind, side: m[2] === "Left" ? "left" : "right" };
}

// Parlato 0.16: the side of each modifier among the keys held, as a trigger
// stores it. Both sides held (or none seen) stays "any".
function sidesFromHeld(held: Set<string>, wanted: Record<ModKind, boolean>): KeySides | undefined {
  const out: KeySides = { ctrl: "any", alt: "any", shift: "any", win: "any" };
  let any = false;
  for (const kind of MOD_KINDS) {
    if (!wanted[kind]) continue;
    let left = false;
    let right = false;
    for (const code of held) {
      const m = modifierOfCode(code);
      if (m?.kind !== kind) continue;
      if (m.side === "left") left = true;
      else right = true;
    }
    const side: KeySide = left && !right ? "left" : right && !left ? "right" : "any";
    out[kind] = side;
    if (side !== "any") any = true;
  }
  return any ? out : undefined;
}

type Captured = {
  label: string;
};

// Parlato: "dictation" = primary/secondary record triggers (any single key,
// text keys need a confirmation); "utility" = additional shortcuts, which are
// swallowed globally, so a text key alone is refused.
export type RecorderRule = "dictation" | "utility";

export function HotkeyRecorder({
  open,
  onCancel,
  onCapture,
  rule = "utility",
}: {
  open: boolean;
  onCancel: () => void;
  onCapture: (trigger: ComboTrigger | ChordTrigger) => void;
  rule?: RecorderRule;
}) {
  const { t } = useTranslation();
  const [captured, setCaptured] = useState<Captured | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState<ComboTrigger | null>(null);
  // Parlato 0.16: store which side (left / right) of each modifier was used.
  const [tellSides, setTellSides] = useState(false);
  const tellSidesRef = useRef(false);
  tellSidesRef.current = tellSides;
  // Modifier keys currently held (event codes), and the biggest set held
  // since the first one went down, to capture "two modifiers alone".
  const heldRef = useRef<Set<string>>(new Set());
  const peakRef = useRef<Set<string>>(new Set());
  const usedFinalRef = useRef(false);
  const finishedRef = useRef(false);
  const pendingRef = useRef(false);
  const dialogRef = useRef<HTMLDivElement>(null);

  // Reset l'etat a chaque ouverture.
  useEffect(() => {
    if (open) {
      setCaptured(null);
      setError(null);
      setPending(null);
      setTellSides(false);
      resetModifiers();
      pendingRef.current = false;
      finishedRef.current = false;
    }
  }, [open]);

  function resetModifiers() {
    heldRef.current = new Set();
    peakRef.current = new Set();
    usedFinalRef.current = false;
  }

  function chooseAnother() {
    setCaptured(null);
    setError(null);
    setPending(null);
    resetModifiers();
    pendingRef.current = false;
    finishedRef.current = false;
    dialogRef.current?.focus();
  }

  useEffect(() => {
    if (!open) return;

    function onKey(e: KeyboardEvent) {
      // Waiting for "Use this key" / "Choose another": leave every key alone
      // so the buttons work from the keyboard. (Escape too for a dictation
      // shortcut, where Esc is a key you can pick: close with Cancel.)
      if (pendingRef.current && (rule === "dictation" || e.key !== "Escape")) return;
      e.preventDefault();
      e.stopPropagation();

      // Parlato 0.16: Escape closes the window, except when recording a
      // dictation shortcut: Esc is allowed there (the Cancel button closes).
      if (
        e.key === "Escape" &&
        rule !== "dictation" &&
        !e.ctrlKey &&
        !e.altKey &&
        !e.shiftKey
      ) {
        onCancel();
        return;
      }

      // Parlato 0.16: remember which modifiers are held, for the side of
      // each one and for "two modifiers alone".
      const mod = modifierOfCode(e.code);
      if (mod) {
        heldRef.current.add(e.code);
        if (heldRef.current.size >= peakRef.current.size) {
          peakRef.current = new Set(heldRef.current);
        }
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
      usedFinalRef.current = true;

      const hasModifier = e.ctrlKey || e.altKey || e.shiftKey || e.metaKey;
      // Utility shortcuts: refuse a text key (letter/digit/space) without a
      // modifier, they are swallowed globally. F-keys and system keys
      // (Insert, Delete, ...) are fine alone. Dictation shortcuts accept any
      // single key.
      if (rule === "utility" && !hasModifier && !STANDALONE_VKS.has(vk)) {
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

      const sides = tellSidesRef.current
        ? sidesFromHeld(heldRef.current, {
            ctrl: e.ctrlKey,
            alt: e.altKey,
            shift: e.shiftKey,
            win: e.metaKey,
          })
        : undefined;
      const trigger: ComboTrigger = {
        kind: "combo",
        vk,
        ctrl: e.ctrlKey,
        alt: e.altKey,
        shift: e.shiftKey,
        win: e.metaKey,
        ...(sides ? { sides } : {}),
      };

      const label = formatCombo(trigger);
      setCaptured({ label });

      // A bare key that types text keeps typing (the dictation key is not
      // swallowed): ask for confirmation instead of closing. Esc too: it is
      // easy to press it just to close the window.
      if (rule === "dictation" && !hasModifier && (typesText(vk) || vk === 0x1b)) {
        pendingRef.current = true;
        setPending(trigger);
        return;
      }

      // On laisse 600ms a l'utilisateur pour voir ce qu'il a capture
      // avant de fermer + commit.
      window.setTimeout(() => onCapture(trigger), 600);
    }

    // Parlato 0.16: two modifiers alone. When the first of them is let go
    // and no other key was pressed in between, the biggest set held (two or
    // more kinds) is the shortcut. Dictation shortcuts only.
    function onKeyUp(e: KeyboardEvent) {
      const mod = modifierOfCode(e.code);
      if (!mod) return;
      if (pendingRef.current || finishedRef.current) return;
      e.preventDefault();
      e.stopPropagation();
      const peak = peakRef.current;
      const wasHeld = heldRef.current.delete(e.code);
      if (rule === "dictation" && wasHeld && !usedFinalRef.current) {
        const kinds = new Set<ModKind>();
        for (const code of peak) {
          const m = modifierOfCode(code);
          if (m) kinds.add(m.kind);
        }
        if (kinds.size >= 2) {
          finishedRef.current = true;
          const wanted = {
            ctrl: kinds.has("ctrl"),
            alt: kinds.has("alt"),
            shift: kinds.has("shift"),
            win: kinds.has("win"),
          };
          const sides = tellSidesRef.current ? sidesFromHeld(peak, wanted) : undefined;
          const trigger: ChordTrigger = { kind: "modifiers", ...wanted, ...(sides ? { sides } : {}) };
          setError(null);
          setCaptured({ label: formatChord(trigger) });
          window.setTimeout(() => onCapture(trigger), 600);
          return;
        }
      }
      if (heldRef.current.size === 0) {
        peakRef.current = new Set();
        usedFinalRef.current = false;
      }
    }

    // A window that loses focus never sees the key-ups.
    function onBlur() {
      resetModifiers();
    }

    document.addEventListener("keydown", onKey, { capture: true });
    document.addEventListener("keyup", onKeyUp, { capture: true });
    window.addEventListener("blur", onBlur);
    return () => {
      document.removeEventListener("keydown", onKey, { capture: true });
      document.removeEventListener("keyup", onKeyUp, { capture: true });
      window.removeEventListener("blur", onBlur);
    };
  }, [open, onCancel, onCapture, rule, t]);

  // Parlato: focus the dialog so screen readers announce it; every key
  // goes to the recorder anyway (captured on the document).
  useEffect(() => {
    if (open) dialogRef.current?.focus();
  }, [open]);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm"
      onClick={onCancel}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="hotkey-record-title"
        aria-describedby="hotkey-record-hint"
        tabIndex={-1}
        className="relative w-full max-w-sm rounded-lg border bg-background p-6 shadow-lg outline-none"
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

        <h2 id="hotkey-record-title" className="text-base font-semibold">
          {t("hotkey.record.title")}
        </h2>
        <p id="hotkey-record-hint" className="mt-1 text-xs text-muted-foreground">
          {t(rule === "dictation" ? "hotkey.record.hintDictation" : "hotkey.record.hint")}
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

        {pending && (
          <p role="alert" className="mt-3 text-xs leading-relaxed text-muted-foreground">
            {t(pending.vk === 0x1b ? "hotkey.record.escWarning" : "hotkey.record.typesTextWarning")}
          </p>
        )}

        {!pending && (
          <label className="mt-3 flex items-center gap-2 text-xs text-muted-foreground">
            <Switch checked={tellSides} onChange={setTellSides} label={t("hotkey.record.tellSides")} />
            <span>{t("hotkey.record.tellSides")}</span>
          </label>
        )}

        <div className="mt-4 flex justify-end gap-2">
          <Button variant="ghost" size="sm" onClick={onCancel}>
            {t("common.cancel")}
          </Button>
          {pending && (
            <>
              <Button variant="outline" size="sm" onClick={chooseAnother}>
                {t("hotkey.record.chooseAnother")}
              </Button>
              <Button size="sm" onClick={() => onCapture(pending)}>
                {t("hotkey.record.useThisKey")}
              </Button>
            </>
          )}
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
  0x13: "pause",
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
  0x5d: "menu",
  0x90: "numLock",
  0x91: "scrollLock",
  0xad: "volumeMute",
  0xae: "volumeDown",
  0xaf: "volumeUp",
  0xb0: "nextTrack",
  0xb1: "prevTrack",
  0xb3: "playPause",
};

// Parlato: Mac-only keys, translated through macOverrides.keys.
const MAC_VK_KEYS: Record<number, string> = {
  0x2d: "help",
  0x0c: "clear",
};

// Parlato: punctuation / numpad labels (US layout names, the same on every
// language).
const SYMBOL_LABELS: Record<number, string> = {
  0xba: ";", 0xbb: "=", 0xbc: ",", 0xbd: "-", 0xbe: ".", 0xbf: "/",
  0xc0: "`", 0xdb: "[", 0xdc: "\\", 0xdd: "]", 0xde: "'", 0xe2: "<>",
  0x6a: "Num *", 0x6b: "Num +", 0x6d: "Num -", 0x6e: "Num .", 0x6f: "Num /",
};

export function vkLabel(vk: number): string {
  // Parlato: WebKit reports the Mac's Right Command as 0x5d, which is the Menu
  // key on Windows; on the Mac it is never a final key, so it never shows up.
  if (isMac && MAC_VK_KEYS[vk]) return i18n.t(`keys.${MAC_VK_KEYS[vk]}`);
  if (VK_KEYS[vk]) return i18n.t(`keys.${VK_KEYS[vk]}`);
  if (vk >= 0x70 && vk <= 0x87) return `F${vk - 0x6f}`; // F1..F24
  if (vk >= 0x60 && vk <= 0x69) return `Num ${vk - 0x60}`; // numpad 0..9
  if (SYMBOL_LABELS[vk]) return SYMBOL_LABELS[vk];
  if (vk >= 0x30 && vk <= 0x39) return String.fromCharCode(vk); // 0..9
  if (vk >= 0x41 && vk <= 0x5a) return String.fromCharCode(vk); // A..Z
  return `VK_${vk.toString(16).toUpperCase().padStart(2, "0")}`;
}

// Parlato 0.16: name of one modifier, with its side when the trigger asks for
// one ("Left Ctrl", "Ctrl gauche"): the same words as the single-key picker.
const SIDE_OPTION: Record<ModKind, { left: string; right: string }> = {
  ctrl: { left: "leftCtrl", right: "rightCtrl" },
  alt: { left: "leftAlt", right: "rightAlt" },
  shift: { left: "leftShift", right: "rightShift" },
  win: { left: "leftWin", right: "rightWin" },
};

function modifierName(kind: ModKind, side: KeySide | undefined): string {
  if (side === "left" || side === "right") {
    return i18n.t(`hotkey.options.${SIDE_OPTION[kind][side]}`);
  }
  return i18n.t(`keys.${kind}`);
}

function modifierParts(
  flags: Record<ModKind, boolean>,
  sides: KeySides | undefined,
): string[] {
  return MOD_KINDS.filter((k) => flags[k]).map((k) => modifierName(k, sides?.[k]));
}

export function formatCombo(combo: ComboTrigger): string {
  const parts = modifierParts(combo, combo.sides);
  parts.push(vkLabel(combo.vk));
  return parts.join(" + ");
}

/** Two or more modifiers alone, e.g. "Ctrl + Win". */
export function formatChord(chord: ChordTrigger): string {
  return modifierParts(chord, chord.sides).join(" + ");
}

/** Readable name of any trigger (sidebar, cards, onboarding). Null for none. */
export function formatTrigger(trigger: HotkeyTrigger | null): string | null {
  if (!trigger || trigger.kind === "none") return null;
  if (trigger.kind === "modifier") return i18n.t(`hotkey.options.${trigger.option}`);
  if (trigger.kind === "modifiers") return formatChord(trigger);
  return formatCombo(trigger);
}
