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

export type CapturePreviewTab = "overview" | "servants" | "craftEssences" | "items";
export interface CapturePreviewQuery {
  tab: CapturePreviewTab;
  search: string;
  classId: number | null;
  rarity: number | null;
  storage: "all" | "inventory" | "storage";
  sort: "number" | "level" | "bond" | "quantity";
  itemCategory: string | null;
  limitBroken: boolean;
  showZero: boolean;
  page: number;
}
export interface CapturePreviewField { label: string; value: string | null }
export interface CapturePreviewItem {
  id: string;
  itemId: number;
  name: string;
  quantity: number;
  category: string;
  icon: string | null;
  note: string | null;
}
export interface CapturePreviewServant {
  instanceId: string;
  svtId: number;
  number: number | null;
  name: string;
  rarity: number | null;
  classId: number | null;
  className: string | null;
  icon: string | null;
  level: number | null;
  npLevel: number | null;
  skills: [number | null, number | null, number | null];
  bond: number | null;
  inStorage: boolean;
  details: CapturePreviewField[];
}
export interface CapturePreviewCeInstance {
  instanceId: string;
  level: number | null;
  limitCount: number | null;
  limitBroken: boolean | null;
  inStorage: boolean;
  locked: boolean | null;
}
export interface CapturePreviewCe {
  svtId: number;
  number: number;
  name: string;
  rarity: number | null;
  icon: string | null;
  quantity: number;
  summary: string;
  instances: CapturePreviewCeInstance[];
}
export interface CapturePreview {
  recordId: string;
  tab: CapturePreviewTab;
  profile: CapturePreviewField[];
  currencies: CapturePreviewItem[];
  fruits: CapturePreviewItem[];
  summary: {
    servantInstances: number | null;
    servantTypes: number | null;
    ceInstances: number | null;
    ceTypes: number | null;
    storageInstances: number | null;
    presents: number | null;
    unclassifiedInstances: number | null;
  };
  classes: { id: number; name: string }[];
  servants: CapturePreviewServant[];
  craftEssences: CapturePreviewCe[];
  items: CapturePreviewItem[];
  total: number;
  page: number;
  pageCount: number;
}
