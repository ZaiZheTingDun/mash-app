import type { Project } from "./project";

export interface ExportableConfigSummary {
  id: string;
  name: string;
  advancedMode: boolean;
  battleSceneCount: number;
  advancedBattleSceneCount: number;
}

export interface ConfigImportPreviewItem {
  importKey: string;
  sourceName: string;
  targetName: string;
  advancedMode: boolean;
  battleSceneCount: number;
  advancedBattleSceneCount: number;
}

export interface ConfigImportInvalidItem {
  label: string;
  reason: string;
}

export interface ConfigImportPreview {
  fileName: string;
  validConfigs: ConfigImportPreviewItem[];
  invalidItems: ConfigImportInvalidItem[];
}

export interface ExportConfigsResult {
  filePath: string;
  exportedCount: number;
}

export interface ConfigImportResult {
  importedProjects: Project[];
}
