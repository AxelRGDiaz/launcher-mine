interface RamSliderProps {
  label: string;
  valueMb: number;
  minMb: number;
  maxMb: number;
  onChange: (valueMb: number) => void;
  warnAboveMb?: number;
}

export function RamSlider({ label, valueMb, minMb, maxMb, onChange, warnAboveMb }: RamSliderProps) {
  const showWarning = warnAboveMb !== undefined && valueMb > warnAboveMb;

  return (
    <div>
      <div className="mb-1 flex items-center justify-between text-sm">
        <span className="text-text-muted">{label}</span>
        <span className="font-medium text-text">{(valueMb / 1024).toFixed(1)} GB</span>
      </div>
      <input
        type="range"
        min={minMb}
        max={maxMb}
        step={256}
        value={valueMb}
        onChange={(e) => onChange(Number(e.target.value))}
        className="w-full accent-[var(--color-primary)]"
      />
      {showWarning && (
        <p className="mt-1 text-xs text-amber-400">
          Asignar más RAM de la recomendada puede ralentizar el resto del sistema.
        </p>
      )}
    </div>
  );
}
