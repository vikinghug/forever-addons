interface Props {
  label: string;
  /** 0–1, or null when the total is unknown. */
  fraction: number | null;
}

export function ProgressBar({ label, fraction }: Props) {
  return (
    <div className="progress" role="status">
      <span>{label}</span>
      <div className="progress-track">
        <div
          className="progress-fill"
          data-indeterminate={fraction === null}
          style={fraction === null ? undefined : { width: `${Math.round(fraction * 100)}%` }}
        />
      </div>
    </div>
  );
}
