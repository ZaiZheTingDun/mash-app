import { invoke } from "../../tauri";
import type { CaptureConfiguration, CaptureStarted, CertificateInstallation, PacketCaptureResponseState } from "../../types/packetCapture";

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
