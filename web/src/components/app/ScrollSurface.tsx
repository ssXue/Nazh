import type { HTMLAttributes } from 'react';

import { useScrollEdgeEffect } from '../../hooks/use-scroll-edge-effect';

export function ScrollSurface({
  children,
  className,
  ...props
}: HTMLAttributes<HTMLDivElement>) {
  const ref = useScrollEdgeEffect<HTMLDivElement>();

  return (
    <div
      ref={ref}
      className={className ? `${className} liquid-scroll-surface` : 'liquid-scroll-surface'}
      {...props}
    >
      {children}
    </div>
  );
}

