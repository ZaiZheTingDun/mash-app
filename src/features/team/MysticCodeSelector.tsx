import { useMemo, useState } from "react";
import { Avatar, Button, Dialog, Flex, Text } from "@radix-ui/themes";
import { convertFileSrc } from "../../tauri";
import { mysticCodeItemPath, type MysticCode } from "../../types/mysticCode";
import type { MysticCodeGender } from "../../types/appUiSettings";

interface MysticCodeSelectorProps {
  codes: MysticCode[] | null;
  selectedId: number | null;
  gender: MysticCodeGender;
  onSelect: (id: number | null) => void;
}

export function MysticCodeSelector({
  codes,
  selectedId,
  gender,
  onSelect,
}: MysticCodeSelectorProps) {
  const [open, setOpen] = useState(false);
  const codeList = useMemo(() => (Array.isArray(codes) ? codes : []), [codes]);
  const selected = useMemo(
    () => codeList.find((code) => code.id === selectedId) ?? null,
    [codeList, selectedId],
  );
  const selectedPath = selected ? mysticCodeItemPath(selected, gender) : null;

  return (
    <>
      <Button
        type="button"
        variant="ghost"
        className="mystic-code-trigger"
        aria-label={selected ? `御主礼装：${selected.name}` : "选择御主礼装"}
        title={selected ? selected.name : "选择御主礼装"}
        onClick={() => setOpen(true)}
      >
        <span className="mystic-code-trigger-image">
          <Avatar
            src={selectedPath ? convertFileSrc(selectedPath) : undefined}
            fallback="礼"
            alt={selected?.name ?? "未选择御主礼装"}
            radius="full"
            size="3"
          />
        </span>
      </Button>

      <Dialog.Root open={open} onOpenChange={setOpen}>
        <Dialog.Content className="mystic-code-dialog">
          <Dialog.Title>选择御主礼装</Dialog.Title>
          <Dialog.Description size="2" color="gray">
            选择后会用于指令设置中的御主礼装技能。
          </Dialog.Description>
          {codeList.length === 0 ? (
            <Text size="2" color="gray">
              未找到御主礼装资源，请在资源管理中更新资源包。
            </Text>
          ) : (
            <div className="mystic-code-grid">
              {codeList.map((code) => {
                const path = mysticCodeItemPath(code, gender);
                return (
                  <button
                    type="button"
                    key={code.id}
                    className={`mystic-code-option${code.id === selectedId ? " selected" : ""}`}
                    aria-label={code.name}
                    aria-pressed={code.id === selectedId}
                    onClick={() => {
                      onSelect(code.id);
                      setOpen(false);
                    }}
                  >
                    <Avatar
                      src={path ? convertFileSrc(path) : undefined}
                      fallback="礼"
                      alt={code.name}
                      radius="full"
                      size="5"
                    />
                    <Text size="1" align="center" className="mystic-code-option-name">
                      {code.name}
                    </Text>
                  </button>
                );
              })}
            </div>
          )}
          {selected && (
            <Flex justify="end" gap="2" mt="3">
              <Button
                type="button"
                variant="soft"
                color="gray"
                onClick={() => {
                  onSelect(null);
                  setOpen(false);
                }}
              >
                取消选择
              </Button>
            </Flex>
          )}
        </Dialog.Content>
      </Dialog.Root>
    </>
  );
}
