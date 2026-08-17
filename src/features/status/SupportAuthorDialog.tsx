import { Button, Dialog, IconButton, Text } from "@radix-ui/themes";
import { Cross1Icon, HeartIcon } from "@radix-ui/react-icons";
import wechatImage from "../../../src-tauri/resources/images/sponsorship/wechat.png";
import alipayImage from "../../../src-tauri/resources/images/sponsorship/alipay.png";

export function SupportAuthorDialog() {
  return (
    <Dialog.Root>
      <Dialog.Trigger>
        <Button
          type="button"
          size="1"
          variant="solid"
          color="gray"
          className="status-support-btn"
        >
          <HeartIcon width={12} height={12} />
          <Text size="1">支持作者</Text>
        </Button>
      </Dialog.Trigger>

      <Dialog.Content maxWidth="720px" className="sponsorship-dialog">
        <Dialog.Close>
          <IconButton
            type="button"
            size="1"
            variant="ghost"
            color="gray"
            className="sponsorship-dialog-close"
            aria-label="关闭支持作者"
          >
            <Cross1Icon width={14} height={14} />
          </IconButton>
        </Dialog.Close>
        <Dialog.Title size="4">支持作者</Dialog.Title>
        <Dialog.Description size="2" color="gray">
          感谢使用，Mash 仍在持续开发中，各位御主也可以通过以下渠道支持作者，谢谢大家的支持。
        </Dialog.Description>

        <div className="sponsorship-code-grid">
          <figure className="sponsorship-code-card">
            <img
              src={wechatImage}
              alt="微信支持作者二维码"
              className="sponsorship-code-image"
            />
            <figcaption>微信</figcaption>
          </figure>
          <figure className="sponsorship-code-card">
            <img
              src={alipayImage}
              alt="支付宝支持作者二维码"
              className="sponsorship-code-image"
            />
            <figcaption>支付宝</figcaption>
          </figure>
        </div>
      </Dialog.Content>
    </Dialog.Root>
  );
}
