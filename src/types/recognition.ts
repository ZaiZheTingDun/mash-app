export type NoblePhantasmDetectionMode = "card" | "gauge";

export interface RecognitionSettings {
  noblePhantasmDetectionMode: NoblePhantasmDetectionMode;
  supportCeThreshold: number;
  supportCeFullGateThreshold: number;
  supportMlbIconThreshold: number;
  supportBondIconThreshold: number;
  stopOnBondLevelUp: boolean;
  stopOnBondMaxLevel: boolean;
  verifySkillActivation: boolean;
}

export interface ProjectRecognitionSettings {
  supportCeThreshold?: number;
  supportCeFullGateThreshold?: number;
  supportMlbIconThreshold?: number;
  supportBondIconThreshold?: number;
  verifySkillActivation?: boolean;
}
