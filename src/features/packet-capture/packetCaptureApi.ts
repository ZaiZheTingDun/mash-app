import { invoke } from "../../tauri";
import type { CaptureConfiguration, CaptureStarted, CertificateInstallation, PacketCaptureResponseState, CaptureHistoryState, PacketCaptureWorkspace } from "../../types/packetCapture";

export function installCaptureCertificate(): Promise<CertificateInstallation> {
  return invoke<CertificateInstallation>("install_capture_certificate");
}

export function configurePacketCapture(): Promise<CaptureConfiguration> {
  return invoke<CaptureConfiguration>("configure_packet_capture");
}

export function startPacketCapture(): Promise<CaptureStarted> {
  return invoke<CaptureStarted>("start_packet_capture");
}

export function getPacketCaptureResponse(): Promise<PacketCaptureResponseState> {
  return invoke<PacketCaptureResponseState>("get_packet_capture_response");
}

export function setupPacketCapture(): Promise<CaptureConfiguration> {
  return invoke<CaptureConfiguration>("setup_packet_capture");
}

export function getPacketCaptureWorkspace(): Promise<PacketCaptureWorkspace> {
  return invoke<PacketCaptureWorkspace>("get_packet_capture_workspace");
}

export function selectPacketCaptureRecord(recordId: string): Promise<CaptureHistoryState> {
  return invoke<CaptureHistoryState>("select_packet_capture_record", { recordId });
}

export function deletePacketCaptureRecord(recordId: string | null): Promise<CaptureHistoryState> {
  return invoke<CaptureHistoryState>("delete_packet_capture_record", { recordId });
}

export function copyPacketCaptureResponse(): Promise<void> {
  return invoke<void>("copy_packet_capture_response");
}

export function stopPacketCapture(): Promise<void> {
  return invoke<void>("stop_packet_capture");
}
