export interface PixelSize {
  width: number;
  height: number;
}

export interface DeviceResolutionStatus {
  serial: string;
  displayId: number;
  physicalSize: PixelSize;
  overrideSize: PixelSize | null;
  effectiveSize: PixelSize;
}

export interface DeviceResolutionResetResult {
  before: DeviceResolutionStatus;
  after: DeviceResolutionStatus | null;
  verified: boolean;
  message: string;
}
