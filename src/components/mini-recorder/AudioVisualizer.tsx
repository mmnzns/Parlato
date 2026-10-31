import { useEffect, useMemo, useRef, useState } from "react";
import { BARS } from "./types";

// -- Audio visualizer (15 bars, wave + center boost) -----------------------

export function AudioVisualizer({ meterDb }: { meterDb: number }) {
  // Normalisation -60dB..0dB -> 0..1 (aligne VoiceInk Recorder.swift)
  const power = normalizePower(meterDb);
  // Curve perceptuelle ^0.7 (VoiceInk AudioVisualizerView amplitude clamp).
  const amplitude = Math.pow(power, 0.7);

  const startTime = useRef<number>(performance.now());
  const [tick, setTick] = useState(0);

  useEffect(() => {
    let raf = 0;
    const loop = () => {
      setTick(performance.now() - startTime.current);
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  }, []);

  const bars = useMemo(() => {
    const t = tick / 1000;
    return Array.from({ length: BARS }, (_, i) => {
      const phase = i * 0.4;
      const wave = Math.sin(t * 8 + phase) * 0.5 + 0.5;
      const distanceFromCenter = Math.abs(i - (BARS - 1) / 2) / ((BARS - 1) / 2);
      const centerBoost = 1.0 - distanceFromCenter * 0.4;
      const height = 4 + amplitude * wave * centerBoost * 24;
      return Math.max(4, Math.min(28, height));
    });
  }, [tick, amplitude]);

  return (
    <div className="flex h-7 items-center gap-[2px]">
      {bars.map((h, i) => (
        <span
          key={i}
          className="w-[3px] rounded-[1.5px] bg-[#B5E61D]"
          style={{ height: `${h}px` }}
        />
      ))}
    </div>
  );
}

export function StaticVisualizer() {
  return (
    <div className="flex h-7 items-center gap-[2px]">
      {Array.from({ length: BARS }).map((_, i) => (
        <span
          key={i}
          className="w-[3px] rounded-[1.5px] bg-white/45"
          style={{ height: "4px" }}
        />
      ))}
    </div>
  );
}

function normalizePower(db: number): number {
  if (!isFinite(db) || db <= -60) return 0;
  const clamped = Math.max(-60, Math.min(0, db));
  return (clamped + 60) / 60;
}
