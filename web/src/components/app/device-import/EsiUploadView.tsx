import { FileJsonIcon, UploadIcon } from '../AppIcons';

export function EsiUploadView({
  file,
  dragOver,
  xml,
  onDragOver,
  onDragLeave,
  onDrop,
  onFileInputChange,
  fileInputRef,
}: {
  file: File | null;
  dragOver: boolean;
  xml: string;
  onDragOver: (e: React.DragEvent) => void;
  onDragLeave: () => void;
  onDrop: (e: React.DragEvent) => void;
  onFileInputChange: (e: React.ChangeEvent<HTMLInputElement>) => void;
  fileInputRef: React.RefObject<HTMLInputElement | null>;
}) {
  return (
    <div className="dm-drawer__pdf-area">
      <div
        className={`dm-drawer__dropzone${dragOver ? ' is-active' : ''}${file ? ' has-file' : ''}`}
        onDragOver={onDragOver}
        onDragLeave={onDragLeave}
        onDrop={onDrop}
        onClick={() => fileInputRef.current?.click()}
      >
        <UploadIcon width={24} height={24} />
        {file ? (
          <div className="dm-drawer__file-info">
            <FileJsonIcon width={16} height={16} />
            <span className="dm-drawer__file-name">{file.name}</span>
            <span className="dm-drawer__file-size">{(file.size / 1024).toFixed(0)} KB</span>
          </div>
        ) : (
          <p>拖拽 .xml / .esi 文件到此处，或点击选择</p>
        )}
      </div>
      <input
        ref={fileInputRef}
        type="file"
        accept=".xml,.esi,text/xml,application/xml"
        hidden
        onChange={onFileInputChange}
      />
      {xml ? (
        <details className="dm-drawer__preview">
          <summary>ESI XML（{xml.length} 字符）</summary>
          <pre className="dm-drawer__preview-text">{xml.slice(0, 5000)}</pre>
        </details>
      ) : null}
    </div>
  );
}
