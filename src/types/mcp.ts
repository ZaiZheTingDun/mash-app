export interface McpServerStatus {
  endpoint: string | null;
  allowBattleRuns: boolean;
}

export type McpClientConfigPart = "address" | "token" | "codex";
