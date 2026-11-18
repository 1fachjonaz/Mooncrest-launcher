const paths: Record<string, string> = {
  home: "M4 11.5 12 5l8 6.5V20a1 1 0 0 1-1 1h-4.5v-5.5h-5V21H5a1 1 0 0 1-1-1z",
  library: "M5 4h3v16H5zM10.5 4h3v16h-3zM16 5.2l2.9-.8 3.1 15.4-2.9.8z",
  discover: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zm3.6 5.4-2.1 5.1-5.1 2.1 2.1-5.1z",
  download: "M12 4v11m0 0-4.5-4.5M12 15l4.5-4.5M5 19.5h14",
  news: "M5 5h11v14H6.5A1.5 1.5 0 0 1 5 17.5zM16 9h3v8.5a1.5 1.5 0 0 1-3 0M8 9h5M8 12.5h5M8 16h3",
  settings: "M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6zm7.4 3a7.6 7.6 0 0 0-.1-1.2l2-1.5-2-3.4-2.3.9a7.4 7.4 0 0 0-2.1-1.2L14.5 3h-5l-.4 2.6A7.4 7.4 0 0 0 7 6.8l-2.3-.9-2 3.4 2 1.5a7.6 7.6 0 0 0 0 2.4l-2 1.5 2 3.4 2.3-.9a7.4 7.4 0 0 0 2.1 1.2l.4 2.6h5l.4-2.6a7.4 7.4 0 0 0 2.1-1.2l2.3.9 2-3.4-2-1.5c.1-.4.1-.8.1-1.2z",
  account: "M12 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8zm-7 8.5c.8-3.4 3.6-5.5 7-5.5s6.2 2.1 7 5.5",
  play: "M8 5.5v13l10.5-6.5z",
  pause: "M8 5.5h2.5v13H8zM13.5 5.5H16v13h-2.5z",
  close: "M6 6l12 12M18 6 6 18",
  folder: "M3.5 7a1.5 1.5 0 0 1 1.5-1.5h4.2l2 2H19a1.5 1.5 0 0 1 1.5 1.5v8.5A1.5 1.5 0 0 1 19 19H5a1.5 1.5 0 0 1-1.5-1.5z",
  refresh: "M19 12a7 7 0 1 1-2.1-5M19 4.5V9h-4.5",
  check: "M5 12.5l4.5 4.5L19 7.5",
  alert: "M12 8v5m0 3.2v.3M10.3 4.3 2.8 17.5A2 2 0 0 0 4.5 20.5h15a2 2 0 0 0 1.7-3L13.7 4.3a2 2 0 0 0-3.4 0z",
  back: "M14.5 6 8.5 12l6 6",
  external: "M14 4.5h5.5V10M19.5 4.5 11 13M17 14v4.5a1 1 0 0 1-1 1H5.5a1 1 0 0 1-1-1V8a1 1 0 0 1 1-1H10",
  minimize: "M6 12h12",
  maximize: "M6.5 6.5h11v11h-11z",
  search: "M10.5 4a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13zM15.5 15.5 20 20",
  file: "M7 3.5h6.5L18 8v12a.5.5 0 0 1-.5.5h-10A.5.5 0 0 1 7 20zM13 3.5V8.5h5",
  trash: "M5 7h14M10 7V5h4v2M7 7l1 13h8l1-13",
  update: "M12 20V9m0 0-4.5 4.5M12 9l4.5 4.5M5 4.5h14",
  more: "M6 12h.01M12 12h.01M18 12h.01",
  grid: "M4.5 4.5h6v6h-6zM13.5 4.5h6v6h-6zM4.5 13.5h6v6h-6zM13.5 13.5h6v6h-6z",
  list: "M8.5 6.5h11M8.5 12h11M8.5 17.5h11M4.5 6.5h.01M4.5 12h.01M4.5 17.5h.01",
  forward: "M9.5 6l6 6-6 6",
};

export type IconName = keyof typeof paths;

export function Icon({ name, size = 18, className }: { name: IconName; size?: number; className?: string }) {
  const filled = name === "play" || name === "pause";
  const sw = name === "more" ? 3 : 1.6;
  return (
    <svg className={className} width={size} height={size} viewBox="0 0 24 24" aria-hidden="true"
      fill={filled ? "currentColor" : "none"} stroke={filled ? "none" : "currentColor"}
      strokeWidth={sw} strokeLinecap="round" strokeLinejoin="round">
      <path d={paths[name]} />
    </svg>
  );
}

/** The Mooncrest mark: a crescent cresting the horizon. */
export function Mark({ size = 28 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 64 64" aria-hidden="true">
      <defs>
        <mask id="mc-c"><rect width="64" height="64" fill="#fff" /><circle cx="38.5" cy="25" r="16" fill="#000" /></mask>
        <clipPath id="mc-a"><rect x="0" y="0" width="64" height="44" /></clipPath>
      </defs>
      <g clipPath="url(#mc-a)"><circle cx="32" cy="32" r="19" fill="var(--moon)" mask="url(#mc-c)" /></g>
      <rect x="10" y="45.5" width="44" height="2.5" rx="1.25" fill="var(--halo)" />
    </svg>
  );
}
