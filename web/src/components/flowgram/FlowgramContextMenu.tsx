/// 画布右键上下文菜单。
///
/// 分两种模式：
/// - 节点右键：复制、删除
/// - 画布空白右键：适配视图、自动整理、全选

import { useCallback, useEffect, useRef, type MouseEvent } from 'react';

import type { FreeLayoutPluginContext } from '@flowgram.ai/free-layout-editor';

export interface ContextMenuState {
  x: number;
  y: number;
  /** 右键目标是节点还是画布空白区。 */
  target: 'node' | 'canvas';
  /** 右键节点的 connection_id（如有）。 */
  connectionId?: string;
}

interface FlowgramContextMenuProps {
  state: ContextMenuState;
  editorCtx: FreeLayoutPluginContext | null;
  onClose: () => void;
}

export function FlowgramContextMenu({ state, editorCtx, onClose }: FlowgramContextMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);

  const handleDuplicate = useCallback(() => {
    if (!editorCtx) return;
    const sel = editorCtx.document.selectServices.selection.filter(
      (n) => n.flowNodeType !== 'GROUP' && !n.disposed,
    );
    const copied = [];
    for (const node of sel) {
      const newNode = editorCtx.document.copyNode(node);
      if (newNode) copied.push(newNode);
    }
    if (copied.length > 0) {
      editorCtx.document.selectServices.selection = copied;
    }
    onClose();
  }, [editorCtx, onClose]);

  const handleDelete = useCallback(() => {
    if (!editorCtx) return;
    const sel = editorCtx.document.selectServices.selection.filter(
      (n) => n.flowNodeType !== 'GROUP' && !n.disposed,
    );
    for (const node of sel) {
      node.dispose();
    }
    editorCtx.document.selectServices.clear();
    onClose();
  }, [editorCtx, onClose]);

  const handleFitView = useCallback(() => {
    if (!editorCtx) return;
    void editorCtx.tools.fitView(true);
    onClose();
  }, [editorCtx, onClose]);

  const handleAutoLayout = useCallback(() => {
    if (!editorCtx) return;
    void editorCtx.tools.autoLayout();
    onClose();
  }, [editorCtx, onClose]);

  const handleSelectAll = useCallback(() => {
    if (!editorCtx) return;
    const nodes = editorCtx.document.getAllNodes().filter(
      (n) => n.flowNodeType !== 'GROUP' && !n.disposed,
    );
    editorCtx.document.selectServices.selection = nodes;
    onClose();
  }, [editorCtx, onClose]);

  // 点击外部或 Escape 关闭
  useEffect(() => {
    function onPointerDown(e: PointerEvent) {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        onClose();
      }
    }
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        onClose();
      }
    }
    document.addEventListener('pointerdown', onPointerDown);
    document.addEventListener('keydown', onKeyDown);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown);
      document.removeEventListener('keydown', onKeyDown);
    };
  }, [onClose]);

  const isNode = state.target === 'node';

  return (
    <div
      ref={menuRef}
      className="flowgram-context-menu"
      style={{ left: state.x, top: state.y }}
      onContextMenu={(e: MouseEvent) => e.preventDefault()}
    >
      {isNode ? (
        <>
          <button type="button" className="flowgram-context-menu__item" onClick={handleDuplicate}>
            复制节点
            <span className="flowgram-context-menu__shortcut">Ctrl+D</span>
          </button>
          <button type="button" className="flowgram-context-menu__item" onClick={handleDelete}>
            删除节点
            <span className="flowgram-context-menu__shortcut">Delete</span>
          </button>
          {state.connectionId ? (
            <>
              <div className="flowgram-context-menu__separator" />
              <div className="flowgram-context-menu__info">
                连接：{state.connectionId}
              </div>
            </>
          ) : null}
        </>
      ) : (
        <>
          <button type="button" className="flowgram-context-menu__item" onClick={handleFitView}>
            适配视图
            <span className="flowgram-context-menu__shortcut">Ctrl+0</span>
          </button>
          <button type="button" className="flowgram-context-menu__item" onClick={handleAutoLayout}>
            自动整理
          </button>
          <div className="flowgram-context-menu__separator" />
          <button type="button" className="flowgram-context-menu__item" onClick={handleSelectAll}>
            全选
            <span className="flowgram-context-menu__shortcut">Ctrl+A</span>
          </button>
        </>
      )}
    </div>
  );
}
