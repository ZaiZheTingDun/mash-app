export interface NormRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface CvTargetSpec {
  template: string;
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
