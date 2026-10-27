export interface CertificateInstallation {
  serial: string;
  alreadyInstalled: boolean;
  fingerprint: string;
  message: string;
}

export interface CaptureConfiguration {
  serial: string;
  packageName: string;
  reqableVersion: string;
  message: string;
}

export interface CaptureStarted {
  serial: string;
  alreadyActive: boolean;
  message: string;
}

export interface PacketCaptureResponseState {
  phase: "idle" | "waiting" | "found" | "error";
  serial: string | null;
  message: string;
  response: {
    recordId: number;
    url: string;
    statusCode: number;
    byteLength: number;
    format: string;
    body: string;
  } | null;
}
