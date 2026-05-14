//! 挂载/滚动触发的入场动画包装器。

import { useEffect, useRef, type ReactNode } from 'react';
import gsap from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';

import { useMotionReduced } from '../../hooks/use-motion-mode';
import './AnimatedContent.css';

gsap.registerPlugin(ScrollTrigger);

interface AnimatedContentProps {
  children: ReactNode;
  distance?: number;
  direction?: 'vertical' | 'horizontal';
  reverse?: boolean;
  duration?: number;
  ease?: string;
  initialOpacity?: number;
  animateOpacity?: boolean;
  scale?: number;
  threshold?: number;
  delay?: number;
  /** 挂载时立即播放，不等待滚动触发（适合面板切换场景）。 */
  triggerOnMount?: boolean;
  className?: string;
  onComplete?: () => void;
}

export function AnimatedContent({
  children,
  distance = 100,
  direction = 'vertical',
  reverse = false,
  duration = 0.8,
  ease = 'power3.out',
  initialOpacity = 0,
  animateOpacity = true,
  scale = 1,
  threshold = 0.1,
  delay = 0,
  triggerOnMount = false,
  className,
  onComplete,
}: AnimatedContentProps) {
  const reduced = useMotionReduced();
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (reduced || !ref.current) return;
    const el = ref.current;
    const axis = direction === 'horizontal' ? 'x' : 'y';
    const offset = reverse ? -distance : distance;

    gsap.set(el, {
      [axis]: offset,
      scale,
      autoAlpha: animateOpacity ? initialOpacity : 1,
    });

    if (triggerOnMount) {
      gsap.to(el, {
        [axis]: 0,
        scale: 1,
        autoAlpha: 1,
        duration,
        ease,
        delay,
        onComplete,
      });
      return;
    }

    ScrollTrigger.create({
      trigger: el,
      start: `top ${100 - threshold * 100}%`,
      onEnter: () => {
        gsap.to(el, {
          [axis]: 0,
          scale: 1,
          autoAlpha: 1,
          duration,
          ease,
          delay,
          onComplete,
        });
      },
    });

    return () => {
      ScrollTrigger.getAll().forEach((st) => {
        if (st.vars.trigger === el) st.kill();
      });
    };
  }, [reduced, distance, direction, reverse, duration, ease, initialOpacity, animateOpacity, scale, threshold, delay, triggerOnMount, onComplete]);

  if (reduced) {
    return <div className={className}>{children}</div>;
  }

  return (
    <div ref={ref} className={`animated-content ${className ?? ''}`}>
      {children}
    </div>
  );
}
