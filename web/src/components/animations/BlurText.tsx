//! 模糊文字入场动画——逐词/逐字从模糊到清晰的入场效果。

import { useEffect, useRef, useState, type CSSProperties } from 'react';
import { motion, useInView } from 'motion/react';

import { useMotionReduced } from '../../hooks/use-motion-mode';

interface BlurTextProps {
  text?: string;
  delay?: number;
  className?: string;
  animateBy?: 'words' | 'letters';
  direction?: 'top' | 'bottom';
  threshold?: number;
  rootMargin?: string;
  stepDuration?: number;
  textAlign?: CSSProperties['textAlign'];
  onAnimationComplete?: () => void;
}

export function BlurText({
  text = '',
  delay = 200,
  className,
  animateBy = 'words',
  direction = 'top',
  threshold = 0.1,
  rootMargin = '0px',
  stepDuration = 0.35,
  textAlign,
  onAnimationComplete,
}: BlurTextProps) {
  const reduced = useMotionReduced();
  const ref = useRef<HTMLParagraphElement>(null);
  const inView = useInView(ref, { once: true, amount: threshold });
  const [hasAnimated, setHasAnimated] = useState(false);

  useEffect(() => {
    if (inView && !hasAnimated) setHasAnimated(true);
  }, [inView, hasAnimated]);

  if (reduced) {
    return <p className={className} style={{ textAlign }}>{text}</p>;
  }

  const units = animateBy === 'words' ? text.split(' ') : text.split('');
  const yBase = direction === 'top' ? 20 : -20;

  return (
    <p ref={ref} className={className} style={{ display: 'flex', flexWrap: 'wrap', textAlign }}>
      {units.map((unit, i) => (
        <motion.span
          key={`${unit}-${i}`}
          initial={{ filter: 'blur(10px)', opacity: 0, y: yBase }}
          animate={
            hasAnimated
              ? { filter: 'blur(0px)', opacity: 1, y: 0 }
              : { filter: 'blur(10px)', opacity: 0, y: yBase }
          }
          transition={{
            duration: stepDuration,
            delay: (i * delay) / 1000,
            ease: 'easeOut',
          }}
          onAnimationComplete={() => {
            if (i === units.length - 1) onAnimationComplete?.();
          }}
          style={{ display: 'inline-block', willChange: 'transform, filter, opacity' }}
        >
          {unit === ' ' ? ' ' : unit}
          {animateBy === 'words' && i < units.length - 1 ? ' ' : ''}
        </motion.span>
      ))}
    </p>
  );
}
