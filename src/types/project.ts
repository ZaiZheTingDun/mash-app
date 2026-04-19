export interface Project {
  id: string;
  name: string;
  /**
   * Servant id pinned for the support-select screen. Mirrors the Rust
   * `Project::support_servant_id` field. `null`/`undefined` means the user
   * hasn't picked a support yet, in which case the runner falls back to
   * tapping the topmost row.
   */
  supportServantId?: number | null;
}
