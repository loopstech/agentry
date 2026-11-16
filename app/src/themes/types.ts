import type { ComponentType } from "react";
import type { Counts, Session } from "../types";

/** What a theme provides. Layout and behaviour are shared; a theme supplies
 *  its stylesheet (scoped under `className`), avatar art, progress style and
 *  wording. */
export interface Theme {
  id: "mono" | "crew";
  name: string;
  className: string;
  /** Avatar for a session, sized by the theme's CSS. `size` is "sm" in the pill. */
  Avatar: ComponentType<{ session: Session; size: "sm" | "md" }>;
  /** Progress indicator under a session row. */
  Progress: ComponentType<{ done: number; total: number }>;
  strings: {
    pillSummary: (c: Counts) => { strong: string; rest: string };
    emptyTitle: string;
    emptyHint: string;
    waitingInTerminal: string;
  };
}
