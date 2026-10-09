import { ChevronUpIcon, ChevronDownIcon } from "@radix-ui/react-icons";

export function ActionRowControls({index,count,disabled=false,onEdit,onMove}: {
  index:number; count:number; disabled?:boolean; onEdit:()=>void; onMove:(direction:-1|1)=>void;
}) {
  return <span className="command-row-controls">
    <button type="button" aria-label={`编辑行动 ${index+1}`} disabled={disabled} onClick={onEdit}>编辑</button>
    <button type="button" aria-label={`上移行动 ${index+1}`} disabled={disabled || index===0} onClick={()=>onMove(-1)}><ChevronUpIcon /></button>
    <button type="button" aria-label={`下移行动 ${index+1}`} disabled={disabled || index+1===count} onClick={()=>onMove(1)}><ChevronDownIcon /></button>
  </span>;
}
