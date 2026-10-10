import { useMemo, useState } from "react";
import { Button, Dialog, Flex, Text } from "@radix-ui/themes";
import { MysticCodeIcon } from "../../components/common/MysticCodeIcon";
import type { MysticCode } from "../../types/mysticCode";
import type { MysticCodeGender } from "../../types/appUiSettings";

interface MysticCodeSelectorProps {
  expanded?: boolean;
  disabled?: boolean;
  codes: MysticCode[] | null;
  selectedId: number | null;
  gender: MysticCodeGender;
  onSelect: (id: number | null) => void;
}

export function MysticCodeSelector({
  codes,
  expanded = false,
  disabled = false,
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

  return (
    <>
      <Button
        type="button"
        disabled={disabled}
        variant="ghost"
        className={`mystic-code-trigger${expanded ? " is-expanded" : ""}`}
        aria-label={selected ? `御主礼装：${selected.name}` : "选择御主礼装"}
        title={selected ? selected.name : "选择御主礼装"}
        onClick={() => setOpen(true)}
      >
        {expanded && <span className="mystic-code-label">MYSTIC CODE</span>}
        <span className="mystic-code-trigger-image">
          <MysticCodeIcon
            code={selected}
            gender={gender}
            label={selected?.name ?? "未选择御主礼装"}
          />
        </span>
        {expanded && <><span className="mystic-code-name">{selected?.name ?? "选择御主礼装"}</span><span className="mystic-code-chevron" aria-hidden="true">⌄</span></>}
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
                    <MysticCodeIcon
                      code={code}
                      gender={gender}
                      label={code.name}
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
