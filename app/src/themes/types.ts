import type { ComponentType } from "react";
import type { Counts, Session } from "../types";

/** What a theme provides. Layout and behaviour are shared; a theme supplies
 *  its stylesheet (scoped under `className`), avatar art, progress style and
 *  wording. Adding a theme = a new folder under themes/ plus one line in
 *  themes/index.ts. */
export interface Theme {
  id: "mono" | "crew";
  name: string;
  /** One line for the Settings theme picker. */
  description: string;
  className: string;
  /** The theme's own accent, used unless the user picks one. */
  defaultAccent: string;
  /** Accent swatches offered in Settings. */
  accents: string[];
  /** Avatar for a session, sized by the theme's CSS. `size` is "sm" in the pill. */
  Avatar: ComponentType<{ session: Session; size: "sm" | "md" }>;
  /** Progress indicator under a session row. */
  Progress: ComponentType<{ done: number; total: number }>;
  /** Small mark before a helper (subagent) name, if the theme has one. */
  HelperIcon?: ComponentType<{ done: boolean }>;
  strings: {
    pillSummary: (c: Counts) => { strong: string; rest: string };
    emptyTitle: string;
    emptyHint: string;
    waitingInTerminal: string;
    allow: string;
    /** Opens the deny form. */
    deny: string;
    denyConfirm: string;
    denyAndStop: string;
    notePlaceholder: string;
    send: string;
    skip: string;
    useTerminal: string;
    /** Heading over the helpers list, if shown. */
    helpersHeading: string | null;
  };
}
