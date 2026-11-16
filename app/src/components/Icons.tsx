// Stroke icons. Decorative: the button carrying one has an aria-label.

const base = {
  width: 14,
  height: 14,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 2,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
};

export const ChevronUp = () => (
  <svg {...base}>
    <path d="M6 15l6-6 6 6" />
  </svg>
);

export const Close = () => (
  <svg {...base}>
    <path d="M6 6l12 12M18 6L6 18" />
  </svg>
);

export const Alert = () => (
  <svg {...base} width={12} height={12} strokeWidth={3}>
    <path d="M12 6v8M12 18v.5" />
  </svg>
);

export const Check = () => (
  <svg {...base} width={11} height={11} strokeWidth={2.5}>
    <path d="M5 12l5 5 9-10" />
  </svg>
);

export const Jump = () => (
  <svg {...base}>
    <path d="M7 17L17 7M9 7h8v8" />
  </svg>
);

export const Sound = () => (
  <svg {...base}>
    <path d="M11 5L6 9H3v6h3l5 4V5z" />
    <path d="M15.5 8.5a5 5 0 0 1 0 7" />
  </svg>
);

export const Muted = () => (
  <svg {...base}>
    <path d="M11 5L6 9H3v6h3l5 4V5z" />
    <path d="M16 9l5 5M21 9l-5 5" />
  </svg>
);
