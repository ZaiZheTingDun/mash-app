export type EmulatorMode = "auto" | "mumu" | "other";

export interface EmulatorOptions {
  supported: boolean;
  mode: EmulatorMode;
  selectedPath: string | null;
  installations: string[];
  scanError: string | null;
  otherOverrideReady: boolean;
}
