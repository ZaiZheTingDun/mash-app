export interface DebugScreenSize {
  w: number;
  h: number;
}

export interface DebugCaptureResult {
  imagePath: string;
  screen: string;
  score: number;
  screenSize: DebugScreenSize | null;
}

export interface DebugStreamStatusDto {
  connected: boolean;
  screenSize: DebugScreenSize | null;
}

export interface DebugStreamFrameResultDto {
  jpegBase64: string;
  width: number;
  height: number;
  screen?: string | null;
  score?: number | null;
  timestampMs: number;
}

export interface NormRectDto {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface ElementMatchDto {
  found: boolean;
  x: number;
  y: number;
  score: number;
  region: NormRectDto | null;
}

export interface ProbeResult {
  label: string;
  threshold: number;
  match: ElementMatchDto;
  timestamp: string;
}

export interface BondLevelUpConfidenceDto {
  anchor?: number;
  bondLevelAfter?: number;
  servantOcr?: number;
  [key: string]: number | undefined;
}

export interface BondLevelUpReadResultDto {
  ok: boolean;
  bondLevelAfter?: number | null;
  servantName?: string | null;
  servantNameMatched?: string | null;
  servantMatchScore?: number | null;
  reason?: string | null;
  confidence?: BondLevelUpConfidenceDto | null;
  diagnostics?: unknown;
}

export interface PointDto {
  x: number;
  y: number;
}

export interface LabeledPointDto {
  label: string;
  point: PointDto;
}

export interface LabeledRegionDto {
  label: string;
  region: NormRectDto;
}

export interface CoordGroupDto {
  id: string;
  label: string;
  points: LabeledPointDto[];
  regions: LabeledRegionDto[];
}

export interface RunnerCoordinatesDto {
  groups: CoordGroupDto[];
}

export interface CommandCardMatchDto {
  slot: number;
  x: number;
  y: number;
  cardRegion: NormRectDto;
  faceRegion: NormRectDto;
  critDigitRegions?: NormRectDto[];
  critDigitReads?: CritDigitReadDto[];
  suit?: "a" | "b" | "q";
  iconScore?: number;
  iconRegion?: NormRectDto;
  servantId?: number;
  isSupport?: boolean;
  isStunned: boolean;
  ascension?: number;
  faceScore?: number;
  critChance?: number;
  supportIconScore?: number;
  supportIconRegion?: NormRectDto;
}

export interface CritDigitReadDto {
  digit: number | null;
  score: number;
  kept: boolean;
}

export interface NoblePhantasmMatchDto {
  slot: number;
  cardRegion: NormRectDto;
  ready: boolean;
  edgeFrac: number;
  stdBgr: number;
  edgeThreshold?: number;
  cardReady?: boolean | null;
  readySource?: "glow" | "gauge" | "unknown" | string;
  gaugeDigitCount?: number | null;
  gaugeHundredsVisible?: boolean | null;
  gaugeDigitModelLabels?: string[] | null;
  turnCountModelLabel?: string | null;
  gaugeSequenceValue?: string | null;
  gaugeSequenceConfidence?: number | null;
  gaugeSequenceAccepted?: boolean | null;
  turnSequenceValue?: string | null;
  turnSequenceConfidence?: number | null;
  turnSequenceAccepted?: boolean | null;
  gaugeRegion?: NormRectDto | null;
  npGlowRegion?: NormRectDto | null;
  npGlowScore?: number | null;
  npGlowReady?: boolean | null;
}

export interface EnhancementServantFaceMatchDto {
  template: string;
  templatePath: string;
  row: number | null;
  col: number | null;
  found: boolean;
  score: number;
  x: number;
  y: number;
  region: NormRectDto | null;
  error?: string;
}

export interface EnhancementServantAnchorDto {
  x: number;
  y: number;
  w: number;
  h: number;
  score: number;
  edgeScore: number;
  grayScore: number;
  source: string;
  row?: number | null;
  col?: number | null;
}

export interface EnhancementServantGridCellDto {
  row: number;
  col: number;
  region: NormRectDto;
}

export interface EnhancementServantDiagnosticsDto {
  failReason?: string | null;
  anchorTemplateKey: string;
  anchorEdgeThreshold: number;
  anchorGrayThreshold: number;
  faceThreshold: number;
  region?: NormRectDto | null;
  anchorCount: number;
  gridCellCount: number;
  attempts: number;
}

export interface EnhancementServantMatchResultDto {
  servantId: number;
  searchRegion: NormRectDto;
  templateCrop: NormRectDto;
  templateSize: { w: number; h: number };
  threshold: number;
  found: boolean;
  x: number;
  y: number;
  score: number;
  best: EnhancementServantFaceMatchDto | null;
  anchors: EnhancementServantAnchorDto[];
  referenceAnchor: EnhancementServantAnchorDto | null;
  gridCells: EnhancementServantGridCellDto[];
  matches: EnhancementServantFaceMatchDto[];
  diagnostics: EnhancementServantDiagnosticsDto;
}

export interface DigitMatchDto {
  value: number;
  score: number;
  region: NormRectDto;
}

/**
 * Snapshot of the runner's attack-button probe (template
 * `Battle.variants.main.elements.attack_button` from cv.json). Surfaces both cv.json
 * template settings and the live match score so the user can tell why automation is stuck on
 * "等待战斗动作…".
 */
export interface AttackButtonResultDto {
  template: string;
  region: NormRectDto;
  threshold: number;
  tapPoint: PointDto;
  found: boolean;
  score: number;
  matchX: number;
  matchY: number;
  matchRegion: NormRectDto | null;
}

export interface BattleSceneResultDto {
  region: NormRectDto;
  scene: number | null;
  total: number | null;
  labelTemplateLoaded: boolean;
  labelThreshold: number;
  digitThreshold: number;
  anchorScore: number;
  anchorBox: NormRectDto | null;
  stripRegion: NormRectDto | null;
  candidates: DigitMatchDto[];
  kept: DigitMatchDto[];
  splitAt: number | null;
  bestGap: number;
  avgWidth: number;
  trimmedLeft: number;
  trimmedRight: number;
  missingDigitTemplates: number[];
  failReason: string | null;
}

export interface SupportCeInfoDto {
  region: NormRectDto;
  score: number;
  passed: boolean;
  threshold: number;
  fullGateScore?: number;
  fullGateThreshold?: number;
  fullGatePassed?: boolean;
  artworkChecks?: SupportCeArtworkCheckDto[];
  iconChecks?: SupportCeIconCheckDto[];
  templatePath?: string;
  error?: string;
}

export interface SupportCeArtworkCheckDto {
  variant: string;
  regionKind: string;
  score: number;
  threshold: number;
  passed: boolean;
  selected: boolean;
  error?: string;
}

export interface SupportCeIconCheckDto {
  kind: string;
  templateKey: string;
  region: NormRectDto;
  score: number;
  passed: boolean;
  threshold: number;
  error?: string;
}

export interface SupportRowMatchDto {
  rowRegion: NormRectDto;
  tap: PointDto;
  nameText: string;
  nameScore: number;
  nameMatchedName?: string | null;
  nameRegion: NormRectDto;
  npText: string;
  npScore: number;
  npRegion: NormRectDto;
  scoreAnchor?: NormRectDto | null;
  scoreRegion?: NormRectDto | null;
  starMapScore?: number | null;
  grandStarMapScore?: number | null;
  scoreText?: string | null;
  scoreConfidence?: number | null;
  scoreFilterPassed?: boolean | null;
  scoreFilterReason?: string | null;
  npMatchedName: string;
  npLevel?: number | null;
  servantLevel?: number | null;
  skillPanel?: "owned" | "append" | null;
  skillLevels?: (number | null)[];
  appendSkillLevels?: (number | null)[];
  skillLevelDiagnostics?: Array<{
    level?: number | null;
    score?: number;
    source?: string;
    region?: NormRectDto;
  }>;
  /**
   * Per-row craft-essence verification, populated only when the user
   * supplies a CE id in the debug toolbar. Lets the overlay draw the
   * CE search window and the score so `SUPPORT_CE_OFFSET_IN_ROW` /
   * `SUPPORT_CE_THRESHOLD` can be calibrated against real captures.
   */
  ce?: SupportCeInfoDto;
  grandCes?: SupportCeInfoDto[];
}

export interface SupportCandidateDto {
  text: string;
  score: number;
  region: NormRectDto;
  matchedName?: string;
}

export interface SupportFragmentDto {
  text: string;
  region: NormRectDto;
  ocrConfidence: number;
  nameScore: number;
  matchedName?: string;
  bestNpScore: number;
  bestNpName: string;
}

export interface SupportDiagnosticsDto {
  listRegion: NormRectDto;
  nameCandidates: SupportCandidateDto[];
  npCandidates: SupportCandidateDto[];
  fragmentCount: number;
  fragments?: SupportFragmentDto[];
  nameOnlyFallback?: boolean;
  nameOnlyReason?: string;
  cvFile?: string;
  cvFingerprint?: string;
  supportSkillContourSplit?: boolean;
  supportRowAnchorSearchRegion?: NormRectDto | null;
  confirmButtonAnchors?: NormRectDto[];
  isGrandSectionVisible?: boolean | null;
  // Per-anchor "冠位从者" ribbon match scores aligned 1-1 with
  // `confirmButtonAnchors`. Each entry is the sidecar's
  // TM_CCOEFF_NORMED max within that row's badge ROI, or `null`
  // when the ROI clipped past the frame edge / the template wasn't
  // loaded. The debug overlay colours each row's box from its own
  // score so a non-Grand row in a partly-Grand list doesn't get
  // false-positively painted as Grand by the aggregate flag.
  grandRibbonAnchorScores?: (number | null)[];
}

export interface FindSupportsResultDto {
  supports: SupportRowMatchDto[];
  diagnostics: SupportDiagnosticsDto;
  scoreFilter?: {
    grandMode: boolean;
    starMapScoreMin?: number | null;
    grandStarMapScoreMin?: number | null;
  };
}
