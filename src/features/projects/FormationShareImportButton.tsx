import { useRef, useState } from "react";
import { Button, Spinner, Text } from "@radix-ui/themes";
import { invoke } from "../../tauri";
import type { Project, ProjectCatalog, ProjectGroup } from "../../types/project";
import { FormationShareImportDialog } from "./FormationShareImportDialog";

interface Props {
  groups?: ProjectGroup[];
  disabled?: boolean;
  onImported: (project: Project) => void;
}

export function FormationShareImportButton({ groups, disabled = false, onImported }: Props) {
  const [open, setOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const [currentGroups, setCurrentGroups] = useState<ProjectGroup[]>(groups ?? []);
  const [error, setError] = useState<string | null>(null);
  const pending = useRef(false);

  const show = async () => {
    if (pending.current) return;
    pending.current = true; setLoading(true); setError(null);
    try {
      const available = groups ?? (await invoke<ProjectCatalog>("get_project_catalog")).groups;
      setCurrentGroups(available);
      setOpen(true);
    } catch (error) {
      setError(String(error));
    } finally {
      pending.current = false; setLoading(false);
    }
  };

  return <>
    <Button type="button" variant="soft" color="gray" disabled={disabled || loading} onClick={() => void show()}>{loading && <Spinner size="1" />}粘贴编队</Button>
    {error && <Text size="1" color="red" role="alert">{error}</Text>}
    {open && <FormationShareImportDialog groups={currentGroups} onClose={() => setOpen(false)} onImported={onImported} />}
  </>;
}
