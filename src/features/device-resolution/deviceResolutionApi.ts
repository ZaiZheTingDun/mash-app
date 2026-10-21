import { invoke } from "../../tauri";
import type { DeviceResolutionResetResult, DeviceResolutionStatus } from "../../types/deviceResolution";

export function getDeviceResolution(): Promise<DeviceResolutionStatus> {
  return invoke("get_device_resolution");
}

export function resetDeviceResolution(serial: string): Promise<DeviceResolutionResetResult> {
  return invoke("reset_device_resolution", { serial });
}
