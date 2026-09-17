export type RankUpQuestMode = "single" | "all";

export interface NormalizedRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface RankUpQuestAnchor {
  x: number;
  y: number;
  w: number;
  h: number;
  score: number;
}

export interface RankUpQuestRow {
  candidateId: string;
  region: NormalizedRect;
  rankUpAnchor: RankUpQuestAnchor | null;
  costAnchor: RankUpQuestAnchor;
  signatureRegions: NormalizedRect[];
  actionable: boolean;
  anchorScore: number;
  meanLuma: number;
  meanSaturation: number;
  meanValue: number;
}

export interface RankUpQuestCaptureResult {
  captureId: string;
  imagePath: string;
  rows: RankUpQuestRow[];
}
