import { useState } from "react";
import { cn } from "@/lib/utils";
import { inputClass } from "./config";

export function FreeTextModel({ value, onCommit }: { value: string; onCommit: (v: string) => void }) {
  const [v, setV] = useState(value);
  return (
    <input
      value={v}
      onChange={(e) => setV(e.target.value)}
      onBlur={() => v.trim() !== value && onCommit(v.trim())}
      className={cn(inputClass, "w-[240px] font-mono")}
    />
  );
}
