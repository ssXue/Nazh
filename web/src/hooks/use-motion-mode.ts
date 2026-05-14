//! 动效模式感知 hook——读取当前动效模式，供动画组件判断是否跳过动画。

import { useSyncExternalStore } from 'react';

/** 订阅 data-motion-mode 属性变更。 */
function subscribe(callback: () => void): () => void {
  const observer = new MutationObserver(callback);
  observer.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ['data-motion-mode'],
  });
  return () => observer.disconnect();
}

/** 返回当前动效模式是否为精简（reduced）。 */
export function useMotionReduced(): boolean {
  return useSyncExternalStore(
    subscribe,
    () => document.documentElement.dataset.motionMode === 'reduced',
    () => false,
  );
}
