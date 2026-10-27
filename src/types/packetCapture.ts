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
    captureId: string;
    capturedAt: number;
    recordId: number;
    url: string;
    statusCode: number;
    byteLength: number;
    format: string;
    body: string;
    items: CapturedItem[];
  } | null;
}

export interface CapturedItem {
  itemId: number;
  name: string;
  quantity: number;
}

export interface CaptureHistoryEntry {
  id: string;
  capturedAt: number;
  serial: string | null;
}

export interface CaptureRecord {
  entry: CaptureHistoryEntry;
  response: NonNullable<PacketCaptureResponseState["response"]>;
}

export interface CaptureHistoryState {
  records: CaptureHistoryEntry[];
  defaultRecordId: string | null;
}

export interface PacketCaptureWorkspace {
  capture: Omit<PacketCaptureResponseState, "response">;
  history: CaptureHistoryState;
}
