import { useCallback, useMemo, useState } from "react";
import { Box, Button, Checkbox, Dialog, Flex, Spinner, Text } from "@radix-ui/themes";
import { invoke } from "../../tauri";
import type {
  ConfigImportPreview,
  ConfigImportResult,
  ExportableConfigSummary,
  ExportConfigsResult,
} from "../../types/dataManagement";
import type { Project } from "../../types/project";

interface SettingsDataManagementPageProps {
  onProjectsImported?: (projects: Project[]) => void;
}

export function SettingsDataManagementPage({
  onProjectsImported,
}: SettingsDataManagementPageProps) {
  const [exportDialogOpen, setExportDialogOpen] = useState(false);
  const [exportableConfigs, setExportableConfigs] = useState<ExportableConfigSummary[]>([]);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(() => new Set());
  const [exportLoading, setExportLoading] = useState(false);
  const [exportError, setExportError] = useState<string | null>(null);
  const [exportMessage, setExportMessage] = useState<string | null>(null);

  const [importDialogOpen, setImportDialogOpen] = useState(false);
  const [importFilePath, setImportFilePath] = useState<string | null>(null);
  const [importPreview, setImportPreview] = useState<ConfigImportPreview | null>(null);
  const [selectedImportKeys, setSelectedImportKeys] = useState<Set<string>>(() => new Set());
  const [importLoading, setImportLoading] = useState(false);
  const [importError, setImportError] = useState<string | null>(null);
  const [importMessage, setImportMessage] = useState<string | null>(null);

  const allSelected = useMemo(
    () => exportableConfigs.length > 0 && selectedIds.size === exportableConfigs.length,
    [exportableConfigs.length, selectedIds.size],
  );
  const allImportSelected = useMemo(
    () =>
      importPreview != null &&
      importPreview.validConfigs.length > 0 &&
      selectedImportKeys.size === importPreview.validConfigs.length,
    [importPreview, selectedImportKeys.size],
  );

  const openExportDialog = useCallback(async () => {
    setExportDialogOpen(true);
    setExportLoading(true);
    setExportError(null);
    setExportMessage(null);
    try {
      const configs = await invoke<ExportableConfigSummary[]>("list_exportable_configs");
      setExportableConfigs(configs);
      setSelectedIds(new Set(configs.map((config) => config.id)));
    } catch (err) {
      setExportError(String(err));
    } finally {
      setExportLoading(false);
    }
  }, []);

  const toggleExportConfig = useCallback((id: string, checked: boolean) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (checked) {
        next.add(id);
      } else {
        next.delete(id);
      }
      return next;
    });
  }, []);

  const toggleAllExportConfigs = useCallback((checked: boolean) => {
    setSelectedIds(checked ? new Set(exportableConfigs.map((config) => config.id)) : new Set());
  }, [exportableConfigs]);

  const handleExport = useCallback(async () => {
    setExportLoading(true);
    setExportError(null);
    setExportMessage(null);
    try {
      const result = await invoke<ExportConfigsResult | null>("export_configs", {
        projectIds: Array.from(selectedIds),
      });
      if (result) {
        setExportMessage(`已导出 ${result.exportedCount} 个配置：${result.filePath}`);
      } else {
        setExportMessage("已取消导出");
      }
      setExportDialogOpen(false);
    } catch (err) {
      setExportError(String(err));
    } finally {
      setExportLoading(false);
    }
  }, [selectedIds]);

  const handlePickImportFile = useCallback(async () => {
    setImportLoading(true);
    setImportError(null);
    setImportMessage(null);
    setImportPreview(null);
    setSelectedImportKeys(new Set());
    setImportFilePath(null);
    try {
      const filePath = await invoke<string | null>("pick_config_import_file");
      if (!filePath) {
        setImportMessage("已取消导入");
        return;
      }
      const preview = await invoke<ConfigImportPreview>("preview_config_import", { filePath });
      setImportFilePath(filePath);
      setImportPreview(preview);
      setSelectedImportKeys(new Set(preview.validConfigs.map((config) => config.importKey)));
      setImportDialogOpen(true);
    } catch (err) {
      setImportError(String(err));
    } finally {
      setImportLoading(false);
    }
  }, []);

  const toggleImportConfig = useCallback((importKey: string, checked: boolean) => {
    setSelectedImportKeys((prev) => {
      const next = new Set(prev);
      if (checked) {
        next.add(importKey);
      } else {
        next.delete(importKey);
      }
      return next;
    });
  }, []);

  const toggleAllImportConfigs = useCallback((checked: boolean) => {
    setSelectedImportKeys(
      checked && importPreview
        ? new Set(importPreview.validConfigs.map((config) => config.importKey))
        : new Set(),
    );
  }, [importPreview]);

  const handleImport = useCallback(async () => {
    if (!importFilePath || !importPreview || selectedImportKeys.size === 0) return;
    setImportLoading(true);
    setImportError(null);
    setImportMessage(null);
    try {
      const result = await invoke<ConfigImportResult>("import_configurations", {
        filePath: importFilePath,
        importKeys: Array.from(selectedImportKeys),
      });
      setImportMessage(`已导入 ${result.importedProjects.length} 个配置`);
      onProjectsImported?.(result.importedProjects);
      setImportDialogOpen(false);
    } catch (err) {
      setImportError(String(err));
    } finally {
      setImportLoading(false);
    }
  }, [importFilePath, importPreview, onProjectsImported, selectedImportKeys]);

  return (
    <Flex direction="column" gap="4">
      <Box className="settings-data-panel">
        <Flex align="start" justify="between" gap="4">
          <Box>
            <Text size="3" weight="bold">
              导入队伍
            </Text>
            <Text size="2" color="gray" className="settings-data-description">
              选择要导入的队伍打包文件
            </Text>
          </Box>
          <Button type="button" variant="soft" color="gray" onClick={handlePickImportFile} disabled={importLoading}>
            {importLoading && !importDialogOpen ? <Spinner size="1" /> : null}
            <Text size="2">导入队伍</Text>
          </Button>
        </Flex>
        {importError && (
          <Text size="2" color="red" className="settings-data-status">
            导入失败：{importError}
          </Text>
        )}
        {importMessage && !importDialogOpen && (
          <Text size="2" color="gray" className="settings-data-status">
            {importMessage}
          </Text>
        )}
      </Box>

      <Box className="settings-data-panel">
        <Flex align="start" justify="between" gap="4">
          <Box>
            <Text size="3" weight="bold">
              导出队伍
            </Text>
            <Text size="2" color="gray" className="settings-data-description">
              将选择的队伍导出到目标文件夹
            </Text>
          </Box>
          <Button type="button" variant="soft" color="gray" onClick={openExportDialog}>
            <Text size="2">导出队伍</Text>
          </Button>
        </Flex>
        {exportMessage && !exportDialogOpen && (
          <Text size="2" color="gray" className="settings-data-status">
            {exportMessage}
          </Text>
        )}
      </Box>

      <Dialog.Root open={exportDialogOpen} onOpenChange={setExportDialogOpen}>
        <Dialog.Content maxWidth="520px">
          <Dialog.Title>导出队伍</Dialog.Title>
          <Flex direction="column" gap="3">
            {exportLoading && exportableConfigs.length === 0 ? (
              <Flex align="center" gap="2">
                <Spinner size="1" />
                <Text size="2" color="gray">
                  正在读取队伍…
                </Text>
              </Flex>
            ) : exportableConfigs.length === 0 ? (
              <Text size="2" color="gray">
                当前没有可导出的队伍。
              </Text>
            ) : (
              <>
                <label className="settings-data-check-row settings-data-check-row-all">
                  <Checkbox
                    checked={allSelected}
                    onCheckedChange={(checked) => toggleAllExportConfigs(checked === true)}
                  />
                  <Text size="2" weight="medium">
                    全选
                  </Text>
                </label>
                <Flex direction="column" gap="2" className="settings-data-list">
                  {exportableConfigs.map((config) => (
                    <label key={config.id} className="settings-data-check-row">
                      <Checkbox
                        checked={selectedIds.has(config.id)}
                        onCheckedChange={(checked) =>
                          toggleExportConfig(config.id, checked === true)
                        }
                      />
                      <Box>
                        <Text size="2" weight="medium">
                          {config.name}
                        </Text>
                      </Box>
                    </label>
                  ))}
                </Flex>
              </>
            )}
            {exportError && (
              <Text size="2" color="red">
                导出失败：{exportError}
              </Text>
            )}
            <Flex justify="end" gap="2">
              <Dialog.Close>
                <Button type="button" variant="soft" color="gray">
                  关闭
                </Button>
              </Dialog.Close>
              <Button
                type="button"
                onClick={handleExport}
                disabled={exportLoading || selectedIds.size === 0}
              >
                {exportLoading && exportableConfigs.length > 0 ? <Spinner size="1" /> : null}
                <Text size="2">导出</Text>
              </Button>
            </Flex>
          </Flex>
        </Dialog.Content>
      </Dialog.Root>

      <Dialog.Root open={importDialogOpen} onOpenChange={setImportDialogOpen}>
        <Dialog.Content maxWidth="520px">
          <Dialog.Title>导入配置</Dialog.Title>
          <Flex direction="column" gap="3">
            {importPreview && (
              <>
                <Text size="2" color="gray">
                  文件：{importPreview.fileName}
                </Text>
                <Box>
                  <Text size="2" weight="bold">
                    将导入
                  </Text>
                  {importPreview.validConfigs.length === 0 ? (
                    <Text size="2" color="gray" className="settings-data-status">
                      没有可导入的配置。
                    </Text>
                  ) : (
                    <>
                      <label className="settings-data-check-row settings-data-check-row-all">
                        <Checkbox
                          checked={allImportSelected}
                          onCheckedChange={(checked) => toggleAllImportConfigs(checked === true)}
                        />
                        <Text size="2" weight="medium">
                          全选
                        </Text>
                      </label>
                      <Flex direction="column" gap="2" className="settings-data-list">
                        {importPreview.validConfigs.map((config) => (
                          <label key={config.importKey} className="settings-data-check-row">
                            <Checkbox
                              checked={selectedImportKeys.has(config.importKey)}
                              onCheckedChange={(checked) =>
                                toggleImportConfig(config.importKey, checked === true)
                              }
                            />
                            <Box>
                              <Text size="2" weight="medium">
                                {config.sourceName} → {config.targetName}
                              </Text>
                            </Box>
                          </label>
                        ))}
                      </Flex>
                    </>
                  )}
                </Box>
                {importPreview.invalidItems.length > 0 && (
                  <Box>
                    <Text size="2" weight="bold" color="red">
                      无法识别
                    </Text>
                    <Flex direction="column" gap="2" className="settings-data-list">
                      {importPreview.invalidItems.map((item, index) => (
                        <Box key={`${item.label}-${index}`} className="settings-data-invalid-row">
                          <Text size="2" weight="medium">
                            {item.label}
                          </Text>
                          <Text size="1" color="red" className="settings-data-meta">
                            {item.reason}
                          </Text>
                        </Box>
                      ))}
                    </Flex>
                  </Box>
                )}
              </>
            )}
            {importError && (
              <Text size="2" color="red">
                导入失败：{importError}
              </Text>
            )}
            <Flex justify="end" gap="2">
              <Dialog.Close>
                <Button type="button" variant="soft" color="gray">
                  关闭
                </Button>
              </Dialog.Close>
              <Button
                type="button"
                onClick={handleImport}
                disabled={
                  importLoading ||
                  !importPreview ||
                  selectedImportKeys.size === 0
                }
              >
                {importLoading && importDialogOpen ? <Spinner size="1" /> : null}
                <Text size="2">导入</Text>
              </Button>
            </Flex>
          </Flex>
        </Dialog.Content>
      </Dialog.Root>
    </Flex>
  );
}
