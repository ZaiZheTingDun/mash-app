export function CommandDraftHeading({ title, hint, onCancel }: {
  title: string;
  hint: string;
  onCancel: () => void;
}) {
  return <>
    <div className="command-draft-heading"><i className="command-diamond" aria-hidden="true" /><strong>{title}</strong><span>{hint}</span></div>
    <button type="button" className="battle-draft-cancel" aria-label="撤销添加行动" onClick={onCancel}>取消</button>
  </>;
}
