//! 鼠标跟随高光卡片——光标悬停时卡片内出现跟随高光。

import { useRef, useCallback, type ReactNode, type ElementType, type HTMLAttributes } from 'react';

import { useMotionReduced } from '../../hooks/use-motion-mode';

interface SpotlightCardProps extends HTMLAttributes<HTMLDivElement> {
  children: ReactNode;
  as?: ElementType;
  spotlightColor?: string;
}

export function SpotlightCard({
  children,
  as: Tag = 'div',
  spotlightColor = 'rgba(255, 255, 255, 0.08)',
  className = '',
  ...props
}: SpotlightCardProps) {
  const reduced = useMotionReduced();
  const cardRef = useRef<HTMLDivElement>(null);

  const handleMouseMove = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      if (reduced || !cardRef.current) return;
      const rect = cardRef.current.getBoundingClientRect();
      const x = e.clientX - rect.left;
      const y = e.clientY - rect.top;
      cardRef.current.style.setProperty('--spotlight-x', `${x}px`);
      cardRef.current.style.setProperty('--spotlight-y', `${y}px`);
      cardRef.current.style.setProperty('--spotlight-color', spotlightColor);
    },
    [reduced, spotlightColor],
  );

  return (
    <Tag
      ref={cardRef as React.Ref<HTMLDivElement>}
      className={`spotlight-card${className ? ` ${className}` : ''}`}
      onMouseMove={handleMouseMove}
      {...props}
    >
      {children}
    </Tag>
  );
}
