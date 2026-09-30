// Parlato: Workbench section card and setting row (docs/design/v1).
//
// <Section> is a card with a numbered header bar ("[01] General", the
// number comes from the page counter in index.css) and a body of rows
// separated by thin dividers. <Row> puts a label and description on the
// left and a control on the right.

import * as React from "react";
import { Card, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { cn } from "@/lib/utils";

export function Section({
  title,
  description,
  action,
  children,
  className,
}: {
  title: React.ReactNode;
  description?: React.ReactNode;
  action?: React.ReactNode;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <Card className={className}>
      <CardHeader className={cn(action && "flex-row items-center justify-between gap-4")}>
        <div className="flex min-w-0 flex-col gap-0.5">
          <CardTitle>{title}</CardTitle>
          {description && <CardDescription>{description}</CardDescription>}
        </div>
        {action && <div className="flex-none">{action}</div>}
      </CardHeader>
      <div className="flex flex-col divide-y">{children}</div>
    </Card>
  );
}

export function Row({
  label,
  description,
  children,
  disabled,
  htmlFor,
  className,
}: {
  label: React.ReactNode;
  description?: React.ReactNode;
  children?: React.ReactNode;
  disabled?: boolean;
  /** When set, clicking the label focuses/toggles that control. */
  htmlFor?: string;
  className?: string;
}) {
  const Label = htmlFor ? "label" : "div";
  return (
    <div
      className={cn(
        "flex items-center gap-5 px-5 py-[13px]",
        disabled && "opacity-50",
        className,
      )}
    >
      <Label
        {...(htmlFor ? { htmlFor } : {})}
        className={cn("flex min-w-0 flex-1 flex-col gap-0.5", htmlFor && "cursor-pointer")}
      >
        <span className="font-medium">{label}</span>
        {description && (
          <span className="text-[13px] leading-[18px] text-pretty text-muted-foreground">
            {description}
          </span>
        )}
      </Label>
      {children && <div className="flex flex-none items-center gap-3">{children}</div>}
    </div>
  );
}

/** Full-width block inside a Section (for content that is not a label/control row). */
export function Block({ children, className }: { children: React.ReactNode; className?: string }) {
  return <div className={cn("px-5 py-4", className)}>{children}</div>;
}

export function Switch({
  id,
  checked,
  onChange,
  disabled,
  label,
}: {
  id?: string;
  checked: boolean;
  onChange: (next: boolean) => void;
  disabled?: boolean;
  label?: string;
}) {
  return (
    <input
      id={id}
      type="checkbox"
      role="switch"
      aria-label={label}
      checked={checked}
      disabled={disabled}
      onChange={(e) => onChange(e.target.checked)}
    />
  );
}

export function Segmented<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
}) {
  return (
    <div className="flex gap-0.5 rounded-sm border-[1.5px] border-edge bg-muted p-0.5" role="radiogroup">
      {options.map((o) => {
        const active = o.value === value;
        return (
          <button
            key={o.value}
            type="button"
            role="radio"
            aria-checked={active}
            onClick={() => onChange(o.value)}
            className={cn(
              "h-7 rounded-sm px-2.5 text-xs font-semibold whitespace-nowrap transition-colors",
              active ? "bg-card text-foreground shadow-[0_0_0_1.5px_var(--edge)]" : "text-muted-foreground hover:text-foreground",
            )}
          >
            {o.label}
          </button>
        );
      })}
    </div>
  );
}

export const selectClass =
  "h-[34px] min-w-[170px] rounded-sm border-[1.5px] border-input bg-background px-3 text-sm";
