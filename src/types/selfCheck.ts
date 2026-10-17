export interface SelfCheckAssetGroup {
  entries: number;
  hasImage: boolean;
  hasJson: boolean;
}

export interface SelfCheckStatus {
  appVersion: string;
  cvRuntimeVersion: string;
  cvRuntimeInstalled: boolean;
  assetVersion: number | null;
  appAssetsVersion: number;
  servants: SelfCheckAssetGroup;
  ces: SelfCheckAssetGroup;
  templates: SelfCheckTemplateGroup[];
}

export interface SelfCheckTemplateGroup {
  server: string;
  checked: number;
  errors: string[];
}
