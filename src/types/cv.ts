export interface NormRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface CvTargetSpec {
  /**
   * Single template key (filename stem under `resources/.../templates/`).
   * Use this for screen `elements` and for screens that only ship one
   * variant of their detect template.
   */
  template?: string;
  /**
   * Multiple template keys treated as alternatives. The detector tries
   * each and keeps the highest score for that screen. Used by screens
   * whose UI ships in multiple skins (e.g. CN's friend-request prompt
   * has both a light and a dark background variant). Either `template`
   * or `templates` must be present; `templates` wins when both are set.
   */
  templates?: string[];
  region?: NormRect;
  threshold?: number;
}

export interface CvScreenSpec {
  detect?: CvTargetSpec;
  elements?: Record<string, CvTargetSpec>;
}

export interface CvConfig {
  screens: Record<string, CvScreenSpec>;
}
