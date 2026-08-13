import { Button, Dialog, Flex, IconButton, Text } from "@radix-ui/themes";
import { Cross2Icon } from "@radix-ui/react-icons";
import type { CraftEssence } from "../../types/craftEssence";
import { AddRowTrigger } from "../../components/common/AddRowTrigger";
import { useCeCards } from "./contentGridAssets";

interface CraftEssenceManageDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  craftEssences: CraftEssence[];
  maxSelections?: number;
  onAdd: () => void;
  onRemove: (craftEssence: CraftEssence) => void;
  onReplace: (craftEssence: CraftEssence) => void;
}

export function CraftEssenceManageDialog({
  open,
  onOpenChange,
  craftEssences,
  maxSelections = 10,
  onAdd,
  onRemove,
  onReplace,
}: CraftEssenceManageDialogProps) {
  const cardSrcById = useCeCards(craftEssences.map((ce) => ce.id));

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Content maxWidth="520px" className="ce-manage-dialog">
        <Flex align="center" justify="between" gap="3">
          <Dialog.Title size="4" mb="0">
            管理礼装
          </Dialog.Title>
          <Text size="2" color="gray">
            {craftEssences.length}/{maxSelections}
          </Text>
        </Flex>
        <Dialog.Description size="2" color="gray" mt="2">
          点击可更改礼装，所有礼装共享满破开关
        </Dialog.Description>

        <div className="ce-manage-list" aria-label="已选择的礼装">
          {craftEssences.map((ce) => {
            const cardSrc = cardSrcById[ce.id];
            return (
              <div className="ce-manage-row" key={ce.id}>
                <IconButton
                  type="button"
                  size="1"
                  variant="soft"
                  color="red"
                  aria-label={`删除礼装 ${ce.name}`}
                  onClick={() => onRemove(ce)}
                >
                  <Cross2Icon />
                </IconButton>
                <button
                  type="button"
                  className="ce-manage-replace"
                  aria-label={`更改礼装 ${ce.name}`}
                  onClick={() => onReplace(ce)}
                >
                  <Text size="1" color="gray" className="ce-manage-id">
                    #{ce.id}
                  </Text>
                  <Text size="2" weight="medium" truncate>
                    {ce.name}
                  </Text>
                  {cardSrc && <img src={cardSrc} alt="" />}
                </button>
              </div>
            );
          })}
          {craftEssences.length < maxSelections && (
            <AddRowTrigger
              className="ce-manage-add-trigger"
              transparentIconBackground
              iconSize={36}
              onClick={onAdd}
            >
              添加礼装
            </AddRowTrigger>
          )}
        </div>

        <Flex justify="end" align="center" mt="4">
          <Dialog.Close>
            <Button type="button">完成</Button>
          </Dialog.Close>
        </Flex>
      </Dialog.Content>
    </Dialog.Root>
  );
}
