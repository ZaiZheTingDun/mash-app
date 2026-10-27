import goldFruit from "../../../src-tauri/resources/images/item_fruit_golden.png";
import silverFruit from "../../../src-tauri/resources/images/item_fruit_silver.png";
import bronzeFruit from "../../../src-tauri/resources/images/item_fruit_bronzed_cobalt.png";
import copperFruit from "../../../src-tauri/resources/images/item_fruit_bronze.png";
import quartz from "../../../src-tauri/resources/images/item_saint_quartz.png";
import type { CapturedItem } from "../../types/packetCapture";

const images: Record<number, string> = { 2: quartz, 100: goldFruit, 101: silverFruit, 104: bronzeFruit, 102: copperFruit };

export function CaptureInventory({ items }: { items: CapturedItem[] }) {
  return <dl className="capture-inventory">
    {items.map(item => <div className="capture-inventory-item" key={item.itemId}>
      {images[item.itemId] ? <img src={images[item.itemId]} alt="" /> : <span className="capture-currency-icon" aria-hidden="true"><i /><i /></span>}
      <div><dt>{item.name}</dt><dd>{item.quantity.toLocaleString("zh-CN")}</dd></div>
    </div>)}
  </dl>;
}
