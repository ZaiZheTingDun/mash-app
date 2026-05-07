import { useCallback, useState } from "react";
import { Button, Spinner, Text } from "@radix-ui/themes";
import { invoke } from "../tauri";

interface AssetBundleImportResult {
  importedServants: boolean;
  importedCraftEssences: boolean;
  servantFiles: number;
  craftEssenceFiles: number;
  installDir: string;
}

interface AssetBundleButtonProps {
  onImported?: () => void;
}

function basename(path: string): string {
  const normalized = path.split("\\").join("/");
  const parts = normalized.split("/");
  return parts[parts.length - 1] || path;
}

export function AssetBundleButton({ onImported }: AssetBundleButtonProps) {
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<string | null>(null);

  const handleImport = useCallback(async () => {
    setBusy(true);
    setStatus(null);
    try {
      const zipPath = await invoke<string | null>("pick_asset_bundle");
      if (!zipPath) {
        return;
      }

      const confirmed = window.confirm(
        `确认导入素材包？\n\n${basename(zipPath)}\n\n现有同名素材将被替换。`
      );
      if (!confirmed) {
        return;
      }

      const result = await invoke<AssetBundleImportResult>("import_asset_bundle", {
        zipPath,
      });
      const parts: string[] = [];
      if (result.importedServants) {
        parts.push(`从者 ${result.servantFiles} 个文件`);
      }
      if (result.importedCraftEssences) {
        parts.push(`礼装 ${result.craftEssenceFiles} 个文件`);
      }
      setStatus(
        parts.length > 0
          ? `导入完成：${parts.join("，")}`
          : "导入完成，但压缩包中没有可用素材"
      );
      onImported?.();
    } catch (err) {
      setStatus(`导入失败：${String(err)}`);
    } finally {
      setBusy(false);
    }
  }, [onImported]);

  return (
    <div className="asset-import-block">
      <Button
        type="button"
        variant="soft"
        color="gray"
        disabled={busy}
        onClick={handleImport}
      >
        {busy ? <Spinner size="1" /> : null}
        <Text size="2" weight="medium">
          {busy ? "导入中…" : "导入素材包"}
        </Text>
      </Button>
      {status && (
        <Text size="1" color={status.startsWith("导入失败") ? "red" : "gray"}>
          {status}
        </Text>
      )}
    </div>
  );
}
