export interface TeamCheckIssue {
  slotIndex: number | null;
  kind: "servant" | "craftEssence" | "mysticCode";
  expectedName: string;
  reason: string;
}

export interface TeamVerificationReport {
  runId: string;
  checkId: number;
  issues: TeamCheckIssue[];
}
