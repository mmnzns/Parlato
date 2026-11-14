// Parlato: small menu for a mode in the list (Duplicate, Rename, Delete).
// Opens on right-click, on the "..." button, or with the keyboard's context
// menu key. Arrow keys move, Enter picks, Escape closes and gives the focus
// back to where it was.

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Copy, Pencil, Trash2 } from "lucide-react";
import { cn } from "@/lib/utils";

export type ModeAction = "duplicate" | "rename" | "delete";

export function ModeMenu({
  x,
  y,
  onPick,
  onClose,
}: {
  x: number;
  y: number;
  onPick: (action: ModeAction) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ left: x, top: y });

  const items: { action: ModeAction; label: string; Icon: typeof Copy; danger?: boolean }[] = [
    { action: "duplicate", label: t("pm.duplicate"), Icon: Copy },
    { action: "rename", label: t("pm.rename"), Icon: Pencil },
    { action: "delete", label: t("pm.delete"), Icon: Trash2, danger: true },
  ];

  // Keep the menu inside the window, then focus the first item.
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    setPos({
      left: Math.max(4, Math.min(x, window.innerWidth - r.width - 4)),
      top: Math.max(4, Math.min(y, window.innerHeight - r.height - 4)),
    });
    el.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
  }, [x, y]);

  useEffect(() => {
    const away = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    document.addEventListener("pointerdown", away, true);
    window.addEventListener("blur", onClose);
    window.addEventListener("resize", onClose);
    return () => {
      document.removeEventListener("pointerdown", away, true);
      window.removeEventListener("blur", onClose);
      window.removeEventListener("resize", onClose);
    };
  }, [onClose]);

  function onKeyDown(e: React.KeyboardEvent) {
    const buttons = Array.from(ref.current?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? []);
    const at = buttons.indexOf(document.activeElement as HTMLElement);
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onClose();
    } else if (e.key === "Tab") {
      onClose();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      buttons[(at + 1) % buttons.length]?.focus();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      buttons[(at - 1 + buttons.length) % buttons.length]?.focus();
    } else if (e.key === "Home") {
      e.preventDefault();
      buttons[0]?.focus();
    } else if (e.key === "End") {
      e.preventDefault();
      buttons[buttons.length - 1]?.focus();
    }
  }

  return (
    <div
      ref={ref}
      role="menu"
      aria-label={t("pm.menuLabel")}
      onKeyDown={onKeyDown}
      style={{ left: pos.left, top: pos.top }}
      className="fixed z-50 flex min-w-[170px] flex-col gap-0.5 rounded-md border-[1.5px] border-edge bg-popover p-1 text-popover-foreground shadow-pop"
    >
      {items.map(({ action, label, Icon, danger }) => (
        <button
          key={action}
          type="button"
          role="menuitem"
          onClick={() => onPick(action)}
          className={cn(
            "flex h-8 items-center gap-2 rounded-sm px-2.5 text-left text-sm font-medium outline-none hover:bg-accent focus-visible:bg-accent",
            danger && "text-destructive",
          )}
        >
          <Icon className="h-3.5 w-3.5" />
          {label}
        </button>
      ))}
    </div>
  );
}
