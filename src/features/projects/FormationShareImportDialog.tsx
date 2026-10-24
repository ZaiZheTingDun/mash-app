import { useEffect, useRef, useState } from "react";
import { Badge, Button, Callout, Dialog, Flex, Select, Spinner, Table, Text, TextArea, TextField } from "@radix-ui/themes";
import type { FormationSharePreview } from "../../types/formationShare";
import type { Project, ProjectGroup } from "../../types/project";
import { formationShareApi } from "./formationShareApi";

interface Props {
  groups?: ProjectGroup[];
  onClose: () => void;
  onImported: (project: Project) => void;
}

/** Mount only while open: drafts and preview tokens belong to one dialog. */
export function FormationShareImportDialog({ groups = [], onClose, onImported }: Props) {
  const [text, setText] = useState("");
  const [preview, setPreview] = useState<FormationSharePreview | null>(null);
  const [name, setName] = useState("");
  const [groupId, setGroupId] = useState("ungrouped");
  const [busy, setBusy] = useState<"preview" | "import" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const request = useRef(0);
  const token = useRef<string | null>(null);
  const pending = useRef(false);

  const discard = (value: string | null) => {
    if (value) void formationShareApi.discard(value).catch(console.error);
  };

  useEffect(() => () => {
    request.current += 1;
    if (token.current) void formationShareApi.discard(token.current).catch(console.error);
  }, []);

  const invalidate = () => {
    request.current += 1;
    discard(token.current);
    token.current = null;
    setPreview(null);
    setError(null);
  };

  const handlePreview = async () => {
    if (pending.current || !text.trim()) return;
    invalidate();
    const generation = request.current;
    pending.current = true;
    setBusy("preview");
    try {
      const result = await formationShareApi.preview(text);
      if (generation !== request.current) {
        discard(result.token);
        return;
      }
      token.current = result.token;
      setPreview(result);
      setName(result.targetName);
    } catch (error) {
      if (generation === request.current) setError(String(error));
    } finally {
      pending.current = false;
      if (generation === request.current) setBusy(null);
    }
  };

  const handleImport = async () => {
    if (!preview || !name.trim() || pending.current) return;
    pending.current = true;
    setBusy("import");
    setError(null);
    try {
      const project = await formationShareApi.import(preview.token, name, groupId === "ungrouped" ? null : groupId);
      token.current = null;
      onImported(project);
      onClose();
    } catch (error) {
      setError(String(error));
    } finally {
      pending.current = false;
      setBusy(null);
    }
  };

  return (
    <Dialog.Root open onOpenChange={(open) => { if (!open && busy !== "import") onClose(); }}>
      <Dialog.Content maxWidth="760px" aria-describedby={undefined}>
        <Dialog.Title>导入编队分享码</Dialog.Title>
        <Flex direction="column" gap="3">
          <TextArea
            aria-label="编队分享码"
            placeholder="粘贴完整分享码"
            value={text}
            disabled={busy !== null}
            rows={3}
            className="formation-share-code"
            onChange={(event) => { invalidate(); setText(event.target.value); }}
          />
          <Flex justify="end">
            <Button type="button" variant="soft" disabled={busy !== null || !text.trim()} onClick={() => void handlePreview()}>
              {busy === "preview" && <Spinner size="1" />}预览
            </Button>
          </Flex>
          {error && <Callout.Root color="red" role="alert"><Callout.Text>{error}</Callout.Text></Callout.Root>}
          {preview && <>
            <Flex align="center" gap="2">
              <Text weight="bold">{preview.sourceName}</Text>
              <Badge color="gray">{preview.advancedMode ? "高级模式" : "普通模式"}</Badge>
            </Flex>
            <div className="formation-share-preview">
              <Table.Root size="1">
                <Table.Header><Table.Row><Table.ColumnHeaderCell>成员</Table.ColumnHeaderCell><Table.ColumnHeaderCell>礼装</Table.ColumnHeaderCell></Table.Row></Table.Header>
                <Table.Body>{preview.members.map((member) => <Table.Row key={member.position}>
                  <Table.Cell>{member.position}. {member.name}{member.isSupport && <Badge size="1" ml="2">助战</Badge>}</Table.Cell>
                  <Table.Cell>{member.craftEssences.length ? member.craftEssences.join(" / ") : "未配置"}{member.mlbRequired && member.isSupport && member.craftEssences.length > 0 ? "（满破）" : ""}</Table.Cell>
                </Table.Row>)}</Table.Body>
              </Table.Root>
              {preview.supportRequirements.length > 0 && <Flex direction="column" gap="1" mt="3">
                <Text size="2" weight="bold">助战要求与执行选项</Text>
                {preview.supportRequirements.map((line, index) => <Text key={index} size="2">{line}</Text>)}
              </Flex>}
              {preview.sections.map((section, index) => <details key={index} className="formation-share-section">
                <summary>{section.title}</summary>
                <Flex direction="column" gap="1" mt="2">{section.lines.length ? section.lines.map((line, i) => <Text key={i} size="2">{line}</Text>) : <Text size="2" color="gray">尚未配置行动</Text>}</Flex>
              </details>)}
            </div>
            {preview.warnings.length > 0 && <Callout.Root color="amber"><Callout.Text>
              {preview.warnings.map((warning, index) => <span key={index} className="formation-share-warning">{warning}</span>)}
            </Callout.Text></Callout.Root>}
            <Flex direction={{ initial: "column", sm: "row" }} gap="3">
              <label className="formation-share-name"><Text as="div" size="2" mb="1">保存名称</Text><TextField.Root aria-label="保存名称" value={name} disabled={busy === "import"} onChange={(event) => setName(event.target.value)} /></label>
              <label><Text as="div" size="2" mb="1">目标分组</Text><Select.Root value={groupId} disabled={busy === "import"} onValueChange={setGroupId}><Select.Trigger aria-label="目标分组" /><Select.Content><Select.Item value="ungrouped">未分组</Select.Item>{groups.map((group) => <Select.Item key={group.id} value={group.id}>{group.name}</Select.Item>)}</Select.Content></Select.Root></label>
            </Flex>
            <Text size="1" color="gray">导入为新队伍，运行设为单次且不自动补充体力。</Text>
          </>}
          <Flex justify="end" gap="2" mt="2">
            <Button type="button" variant="soft" color="gray" disabled={busy === "import"} onClick={onClose}>取消</Button>
            <Button type="button" disabled={!preview || !name.trim() || busy !== null} onClick={() => void handleImport()}>{busy === "import" && <Spinner size="1" />}导入为新队伍</Button>
          </Flex>
        </Flex>
      </Dialog.Content>
    </Dialog.Root>
  );
}
