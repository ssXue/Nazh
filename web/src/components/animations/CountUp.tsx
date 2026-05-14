//! 数字计数动画——从 from 到 to 的弹簧计数器。

import { useEffect, useRef, type CSSProperties } from 'react';
import { useMotionValue, useSpring, useInView } from 'motion/react';

import { useMotionReduced } from '../../hooks/use-motion-mode';

interface CountUpProps {
  to: number;
  from?: number;
  direction?: 'up' | 'down';
  delay?: number;
  duration?: number;
  className?: string;
  style?: CSSProperties;
  startWhen?: boolean;
  separator?: string;
  onStart?: () => void;
  onEnd?: () => void;
}

export function CountUp({
  to,
  from = 0,
  direction = 'up',
  delay = 0,
  duration = 2,
  className,
  style,
  startWhen = true,
  separator,
  onStart,
  onEnd,
}: CountUpProps) {
  const reduced = useMotionReduced();
  const ref = useRef<HTMLSpanElement>(null);
  const inView = useInView(ref, { once: true });
  const motionVal = useMotionValue(direction === 'down' ? to : from);
  const spring = useSpring(motionVal, { duration: duration * 1000, bounce: 0 });

  useEffect(() => {
    if (reduced || !startWhen || !inView) return;
    const timer = setTimeout(() => {
      onStart?.();
      motionVal.set(direction === 'down' ? from : to);
    }, delay * 1000);
    return () => clearTimeout(timer);
  }, [reduced, startWhen, inView, motionVal, direction, from, to, delay, onStart]);

  useEffect(() => {
    if (reduced) return;
    const update = (v: number) => {
      if (!ref.current) return;
      ref.current.textContent = formatValue(v, separator);
    };
    update(spring.get());
    const unsub = spring.on('change', (v) => {
      update(v);
      if (Math.abs(v - (direction === 'down' ? from : to)) < 0.5) {
        onEnd?.();
      }
    });
    return unsub;
  }, [reduced, spring, direction, from, to, separator, onEnd]);

  if (reduced) {
    return (
      <span className={className} style={style}>
        {formatValue(to, separator)}
      </span>
    );
  }

  return <span ref={ref} className={className} style={style}>{formatValue(direction === 'down' ? to : from, separator)}</span>;
}

function formatValue(v: number, separator?: string): string {
  const rounded = Math.round(v);
  if (!separator) return String(rounded);
  return rounded.toLocaleString('en-US');
}
