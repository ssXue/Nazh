import { useEffect, useRef } from 'react';

export function useScrollEdgeEffect<T extends HTMLElement>() {
  const ref = useRef<T | null>(null);

  useEffect(() => {
    const element = ref.current;
    if (!element) {
      return;
    }

    const updateScrollState = () => {
      const hasOverflow = element.scrollHeight - element.clientHeight > 6;
      const showTopEdge = element.scrollTop > 6;
      const showBottomEdge = element.scrollTop + element.clientHeight < element.scrollHeight - 6;

      element.dataset.scrollActive = hasOverflow ? 'true' : 'false';
      element.dataset.scrollTop = showTopEdge ? 'true' : 'false';
      element.dataset.scrollBottom = showBottomEdge ? 'true' : 'false';
      element.style.setProperty('--scroll-edge-top-opacity', showTopEdge ? '1' : '0');
      element.style.setProperty('--scroll-edge-bottom-opacity', showBottomEdge ? '1' : '0');
    };

    updateScrollState();

    const resizeObserver = new ResizeObserver(() => {
      updateScrollState();
    });
    resizeObserver.observe(element);

    element.addEventListener('scroll', updateScrollState, { passive: true });
    window.addEventListener('resize', updateScrollState);

    return () => {
      resizeObserver.disconnect();
      element.removeEventListener('scroll', updateScrollState);
      window.removeEventListener('resize', updateScrollState);
    };
  }, []);

  return ref;
}
