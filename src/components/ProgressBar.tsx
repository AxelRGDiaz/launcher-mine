interface ProgressBarProps {
  /** 0-1. Si es null, se muestra en modo indeterminado (animación continua). */
  ratio: number | null;
  label?: string;
}

export function ProgressBar({ ratio, label }: ProgressBarProps) {
  const percent = ratio === null ? null : Math.max(0, Math.min(100, Math.round(ratio * 100)));

  return (
    <div className="w-full">
      {label && <div className="mb-1 truncate text-xs text-text-muted">{label}</div>}
      <div className="h-2 w-full overflow-hidden rounded-full bg-surface-sunken">
        <div
          className={`h-full rounded-full bg-primary transition-[width] duration-150 ${
            percent === null ? "w-1/3 animate-pulse" : ""
          }`}
          style={percent === null ? undefined : { width: `${percent}%` }}
        />
      </div>
    </div>
  );
}
