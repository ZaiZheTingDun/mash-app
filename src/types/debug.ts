export type ImageRecognitionDebugMode = "enabled" | "shadow" | "disabled";

export interface DebugSettings {
  imageRecognitionDebugMode: ImageRecognitionDebugMode;
  sequenceRecognitionDebugMode: ImageRecognitionDebugMode;
  autoCaptureBattleBeforeAttack: boolean;
  autoCaptureBattleResultLoot: boolean;
  autoCaptureUnknownScreenTimeout: boolean;
  autoCaptureSkillUseProbe: boolean;
  autoCaptureUnrecognizedCriticalChance: boolean;
  simulateStuckAttackSelection: boolean;
}
