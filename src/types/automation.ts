export type AutomationStatus = "idle" | "starting" | "running" | "finished" | "error";

export interface AutomationStatusPayload {
  status: AutomationStatus;
}

export function isAutomationRunning(payload: AutomationStatusPayload): boolean {
  return payload.status === "starting" || payload.status === "running";
}

export function isAutomationTerminal(payload: AutomationStatusPayload): boolean {
  return payload.status === "idle" || payload.status === "finished" || payload.status === "error";
}
