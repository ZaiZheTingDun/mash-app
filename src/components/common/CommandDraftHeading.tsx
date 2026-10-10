export function CommandDraftHeading({ title, hint, onCancel, onReselect, reselectLabel = "重选" }: {
  title: string;
  hint: string;
  onCancel: () => void;
  onReselect?: () => void;
  reselectLabel?: string;
}) {
  return <>
    <div className="command-draft-heading"><i className="command-diamond" aria-hidden="true" /><strong>{title}</strong><span>{hint}</span></div>
    {onReselect && <button type="button" className="command-draft-reselect" onClick={onReselect}>{reselectLabel}</button>}
    <button type="button" className="battle-draft-cancel" aria-label="撤销添加行动" onClick={onCancel}>取消</button>
  </>;
}
