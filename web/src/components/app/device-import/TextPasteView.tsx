import { SparklesIcon } from '../AppIcons';

export function TextPasteView({
  value,
  onChange,
  extracting,
  onExtract,
}: {
  value: string;
  onChange: (v: string) => void;
  extracting: boolean;
  onExtract: () => void;
}) {
  return (
    <div className="dm-drawer__text-area">
      <textarea
        className="dm-drawer__textarea"
        placeholder="粘贴设备说明书文本..."
        value={value}
        onChange={(e) => onChange(e.target.value)}
        rows={10}
        disabled={extracting}
      />
      <div className="dm-drawer__actions">
        <button
          type="button"
          className="dm-drawer__extract-btn"
          disabled={extracting || !value.trim()}
          onClick={onExtract}
        >
          <SparklesIcon width={14} height={14} />
          AI 抽取
        </button>
      </div>
    </div>
  );
}
