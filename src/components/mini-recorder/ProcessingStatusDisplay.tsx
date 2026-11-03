import { useEffect, useState } from "react";
import { cn } from "@/lib/utils";

// -- Processing display ("Transcribing" / "Enhancing" + 5 dots) ------------

export function ProcessingStatusDisplay({
  label,
  intervalMs,
}: {
  label: string;
  intervalMs: number;
}) {
  const [step, setStep] = useState(0);
  useEffect(() => {
    const id = window.setInterval(() => setStep((s) => (s + 1) % 6), intervalMs);
    return () => window.clearInterval(id);
  }, [intervalMs]);
  return (
    <div className="flex items-center gap-2 whitespace-nowrap text-[11px] font-medium text-white">
      <span>{label}</span>
      <span className="flex items-center gap-[3px]">
        {Array.from({ length: 5 }).map((_, i) => (
          <span
            key={i}
            className={cn(
              "h-[3px] w-[3px] rounded-full transition-opacity",
              i < step ? "bg-white" : "bg-white/30",
            )}
          />
        ))}
      </span>
    </div>
  );
}
