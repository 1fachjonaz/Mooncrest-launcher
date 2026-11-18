/**
 * The launcher's signature: progress drawn along the limb of a rising moon.
 * The arc is the upper half of a circle; it fills from the horizon on the left.
 */
export function Crest({ value, size = 56, label }: { value: number | null; size?: number; label?: string }) {
  const r = 26;
  const len = Math.PI * r;
  const v = value == null ? 0 : Math.max(0, Math.min(1, value));
  return (
    <svg className={`crest ${value == null ? "is-indeterminate" : ""}`} width={size} height={size * 0.6}
      viewBox="0 0 64 38" role="progressbar" aria-label={label}
      aria-valuemin={0} aria-valuemax={100} aria-valuenow={value == null ? undefined : Math.round(v * 100)}>
      <path className="crest-track" d={`M6 32 A${r} ${r} 0 0 1 58 32`} />
      <path className="crest-fill" d={`M6 32 A${r} ${r} 0 0 1 58 32`} strokeDasharray={`${len}`}
        strokeDashoffset={value == null ? len * 0.75 : len * (1 - v)} />
      <line className="crest-horizon" x1="0" y1="34.5" x2="64" y2="34.5" />
    </svg>
  );
}

/** Thin horizontal progress with the same visual language. */
export function Horizon({ value }: { value: number | null }) {
  const v = value == null ? null : Math.max(0, Math.min(1, value));
  return (
    <div className={`horizon ${v == null ? "is-indeterminate" : ""}`} role="progressbar"
      aria-valuemin={0} aria-valuemax={100} aria-valuenow={v == null ? undefined : Math.round(v * 100)}>
      <div className="horizon-fill" style={v == null ? undefined : { width: `${v * 100}%` }} />
    </div>
  );
}
