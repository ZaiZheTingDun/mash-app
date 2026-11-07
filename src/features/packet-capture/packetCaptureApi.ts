import { invoke } from "../../tauri";
import type { CaptureAppList, CaptureClientContext, CaptureConfiguration, CaptureStarted, CertificateInstallation, PacketCaptureResponseState, CaptureHistoryState, CaptureRecord, PacketCaptureWorkspace } from "../../types/packetCapture";

export function installCaptureCertificate(): Promise<CertificateInstallation> {
  return invoke<CertificateInstallation>("install_capture_certificate");
}

export function configurePacketCapture(): Promise<CaptureConfiguration> {
  return invoke<CaptureConfiguration>("configure_packet_capture");
}

export function startPacketCapture(clientContext?: CaptureClientContext): Promise<CaptureStarted> {
  return invoke<CaptureStarted>("start_packet_capture", clientContext ? { clientContext } : undefined);
}

export function getPacketCaptureResponse(): Promise<PacketCaptureResponseState> {
  return invoke<PacketCaptureResponseState>("get_packet_capture_response");
}

export function setupPacketCapture(clientContext?: CaptureClientContext): Promise<CaptureConfiguration> {
  return invoke<CaptureConfiguration>("setup_packet_capture", clientContext ? { clientContext } : undefined);
}

export function getPacketCaptureWorkspace(): Promise<PacketCaptureWorkspace> {
  return invoke<PacketCaptureWorkspace>("get_packet_capture_workspace");
}

export function selectPacketCaptureRecord(recordId: string): Promise<CaptureRecord> {
  return invoke<CaptureRecord>("select_packet_capture_record", { recordId });
}

export function deletePacketCaptureRecord(recordId: string | null): Promise<CaptureHistoryState> {
  return invoke<CaptureHistoryState>("delete_packet_capture_record", { recordId });
}

export function copyPacketCaptureResponse(recordId: string): Promise<void> {
  return invoke<void>("copy_packet_capture_response", { recordId });
}

export function exportPacketCaptureResponse(recordId: string): Promise<string | null> {
  return invoke<string | null>("export_packet_capture_response", { recordId });
}

export function stopPacketCapture(): Promise<void> {
  return invoke<void>("stop_packet_capture");
}

export function getPacketCaptureApps(): Promise<CaptureAppList> {
  return invoke<CaptureAppList>("get_packet_capture_apps");
}

export function selectPacketCaptureApp(list: CaptureAppList, packageName: string): Promise<CaptureAppList> {
  return invoke<CaptureAppList>("select_packet_capture_app", { serial: list.serial, server: list.server, packageName });
}
