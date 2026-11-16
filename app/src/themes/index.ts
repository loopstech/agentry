import type { CSSProperties } from "react";
import type { Config } from "../types";
import type { Theme } from "./types";
import { mono } from "./mono";
import { crew } from "./crew";

export const THEMES: Theme[] = [mono, crew];

export function themeFor(config: Config | null): Theme {
  return THEMES.find((t) => t.id === config?.appearance.theme) ?? mono;
}

/** Black or white text, whichever reads better on `hex`. */
export function textOn(hex: string): string {
  const n = parseInt(hex.slice(1), 16);
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((c) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  const luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
  return luminance > 0.4 ? "#0f1030" : "#ffffff";
}

/** CSS variables for the user's accent colour, if they picked one. */
export function accentStyle(theme: Theme, config: Config | null): CSSProperties {
  const accent = config?.appearance.accent ?? theme.defaultAccent;
  return { "--accent": accent, "--on-accent": textOn(accent) } as CSSProperties;
}
