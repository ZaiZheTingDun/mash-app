export interface SelfCheckAssetGroup {
  entries: number;
  hasImage: boolean;
  hasJson: boolean;
}

export interface SelfCheckStatus {
  appVersion: string;
  cvRuntimeVersion: string;
  cvRuntimeInstalled: boolean;
  cvCodeVersion: string;
  cvCodeInstalled: boolean;
  assetVersion: number | null;
  appAssetsVersion: number;
  servants: SelfCheckAssetGroup;
  ces: SelfCheckAssetGroup;
}
