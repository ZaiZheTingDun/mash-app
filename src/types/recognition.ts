export type NoblePhantasmDetectionMode = "card" | "gauge" | "gaugeBeforeAttack";

export interface RecognitionSettings {
  autoFriendRequest: boolean;
  noblePhantasmDetectionMode: NoblePhantasmDetectionMode;
  supportCeThreshold: number;
  supportCeFullGateThreshold: number;
  supportMlbIconThreshold: number;
  supportBondIconThreshold: number;
  stopOnBondLevelUp: boolean;
  stopOnBondMaxLevel: boolean;
  autoCaptureBondLevelUp: boolean;
  verifySkillActivation: boolean;
  enableExtraClassFilter: boolean;
  supportFullListOcrFallback: boolean;
  unknownScreenTimeoutCount: number;
}

export type SupportClassFilterMode = "correspondingClass" | "allClasses" | "both";

export interface ProjectRecognitionSettings {
  supportClassFilterMode?: SupportClassFilterMode;
  supportCeThreshold?: number;
  supportCeFullGateThreshold?: number;
  supportMlbIconThreshold?: number;
  supportBondIconThreshold?: number;
  verifySkillActivation?: boolean;
  enableExtraClassFilter?: boolean;
  stopOnFiveStarCeDrop?: boolean;
  fiveStarCeDropTargetCount?: number;
}
