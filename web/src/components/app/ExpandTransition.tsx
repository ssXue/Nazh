import type { ReactNode } from 'react';

interface ExpandTransitionProps {
  /** 是否展开覆盖层。 */
  active: boolean;
  /** 加载中态（显示在覆盖层中央）。 */
  loading?: boolean;
  /** 覆盖层定位模式：全屏或居中浮层。 */
  mode?: 'fullscreen' | 'centered';
  /** 底层视图（卡片网格 / 列表）。 */
  base: ReactNode;
  /** 覆盖层内容（详情面板 / 编辑器）。 */
  overlay: ReactNode;
}

/**
 * 展开/收起过渡动画容器。
 *
 * 底层视图淡出 + 模糊，覆盖层从中心放大进入。
 * 被 DeviceModelingPanel / BoardsPanel / ConnectionStudio 复用。
 */
export function ExpandTransition({
  active,
  loading,
  mode = 'fullscreen',
  base,
  overlay,
}: ExpandTransitionProps) {
  return (
    <>
      <div className={`expand-base${active ? ' is-hidden' : ''}`}>
        {base}
      </div>
      {loading && (
        <div className={`expand-overlay${mode === 'centered' ? ' expand-overlay--centered' : ''}`}>
          <div className="expand-overlay__loading">加载中...</div>
        </div>
      )}
      {active && !loading && (
        <div className={`expand-overlay${mode === 'centered' ? ' expand-overlay--centered' : ''}`}>
          {overlay}
        </div>
      )}
    </>
  );
}
