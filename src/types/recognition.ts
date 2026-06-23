export type NoblePhantasmDetectionMode = "card" | "gauge";

export interface RecognitionSettings {
  noblePhantasmDetectionMode: NoblePhantasmDetectionMode;
  supportCeThreshold: number;
  supportCeFullGateThreshold: number;
  supportMlbIconThreshold: number;
  supportBondIconThreshold: number;
}

export interface ProjectRecognitionSettings {
  supportCeThreshold?: number;
  supportCeFullGateThreshold?: number;
  supportMlbIconThreshold?: number;
  supportBondIconThreshold?: number;
}
