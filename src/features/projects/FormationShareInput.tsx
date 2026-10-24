import { useEffect, useState } from "react";
import { Box, Button, Callout, Flex, Spinner, Text, TextArea, Tooltip } from "@radix-ui/themes";
import { BattleActorIcon } from "../../components/common/BattleActorIcon";
import { convertFileSrc, invoke } from "../../tauri";
import type { FormationShareMember, FormationSharePreview } from "../../types/formationShare";

interface Props {
  text: string;
  preview: FormationSharePreview | null;
  busy: "preview" | "import" | null;
  error: string | null;
  onChange: (text: string) => void;
  onRetry: () => void;
}

function MemberAvatar({ member }: { member: FormationShareMember }) {
  const [source, setSource] = useState<string | undefined>();
  const { servantId, servantVariantKey, faceId } = member;
  useEffect(() => {
    if (servantId === null) return;
    let cancelled = false;
    void invoke<string | null>("get_servant_face_path", {
      servantId, variantKey: servantVariantKey, faceId,
    }).then((path) => {
      if (!cancelled) setSource(path ? convertFileSrc(path) : undefined);
    }).catch(() => { if (!cancelled) setSource(undefined); });
    return () => { cancelled = true; };
  }, [servantId, servantVariantKey, faceId]);

  const label = `第${member.position}位 · ${member.name}${member.isSupport ? "（助战）" : ""}`;
  return <Tooltip content={label}>
    <Box role="listitem" aria-label={label} flexShrink="0">
      <BattleActorIcon kind="servant" src={source} label={label} isSupport={member.isSupport} />
    </Box>
  </Tooltip>;
}

export function FormationShareInput({ text, preview, busy, error, onChange, onRetry }: Props) {
  return <Flex direction="column" gap="3">
    <label>
      <Text as="div" size="2" mb="2" weight="medium">编队分享码（可选）</Text>
      <TextArea
        aria-label="编队分享码"
        placeholder="粘贴分享码，从已有编队新建队伍"
        value={text}
        disabled={busy === "import"}
        rows={3}
        className="formation-share-code"
        onChange={(event) => onChange(event.target.value)}
      />
    </label>
    {busy === "preview" && <Flex align="center" gap="2" role="status"><Spinner size="1" /><Text size="2" color="gray">正在读取编队…</Text></Flex>}
    {error && <Callout.Root color="red" role="alert">
      <Flex direction="column" align="start" gap="2">
        <Callout.Text>{error}</Callout.Text>
        <Button type="button" size="1" variant="soft" disabled={busy !== null} onClick={onRetry}>重新读取</Button>
      </Flex>
    </Callout.Root>}
    {preview && <div role="list" aria-label="编队成员" className="formation-share-members">
      {preview.members.map((member) => <MemberAvatar key={`${member.position}:${member.servantId}:${member.servantVariantKey}:${member.faceId}`} member={member} />)}
    </div>}
  </Flex>;
}
