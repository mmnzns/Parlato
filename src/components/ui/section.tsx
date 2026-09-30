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

/** Numbered heading for content that is not a card (e.g. a row of choice cards). */
export function SectionHeading({
  title,
  description,
  action,
}: {
  title: React.ReactNode;
  description?: React.ReactNode;
  action?: React.ReactNode;
}) {
  return (
    <div data-slot="card-header" className="flex items-end justify-between gap-4">
      <div className="flex min-w-0 flex-col gap-0.5">
        <h2 data-slot="card-title" className="flex items-center gap-2 font-display text-base leading-5 font-bold">
          {title}
        </h2>
        {description && (
          <p className="text-[13px] leading-[18px] text-pretty text-muted-foreground">{description}</p>
        )}
      </div>
      {action && <div className="flex-none">{action}</div>}
    </div>
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

/** One option in a radio list inside a Section: dot, title, description. */
export function RadioRow({
  selected,
  onSelect,
  title,
  description,
  trailing,
  disabled,
}: {
  selected: boolean;
  onSelect: () => void;
  title: React.ReactNode;
  description?: React.ReactNode;
  trailing?: React.ReactNode;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={selected}
      disabled={disabled}
      onClick={onSelect}
      className={cn(
        "flex w-full items-start gap-3.5 px-5 py-[13px] text-left transition-colors disabled:cursor-not-allowed disabled:opacity-60",
        selected ? "bg-accent" : "hover:bg-muted/60",
      )}
    >
      <span
        className={cn(
          "mt-0.5 flex h-[18px] w-[18px] flex-none items-center justify-center rounded-full border-[1.5px] bg-card",
          selected ? "border-foreground" : "border-input",
        )}
      >
        <span className={cn("h-2 w-2 rounded-full bg-foreground", !selected && "opacity-0")} />
      </span>
      <span className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="font-medium">{title}</span>
        {description && (
          <span className="text-[13px] leading-[18px] text-pretty text-muted-foreground">{description}</span>
        )}
      </span>
      {trailing && <span className="flex-none self-center">{trailing}</span>}
    </button>
  );
}
