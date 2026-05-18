import type { ReactNode } from 'react';

import { ScrollSurface } from './ScrollSurface';

interface PanelPageShellProps {
  pageKey: string;
  children: ReactNode;
  className?: string;
  scrollClassName?: string;
}

export function PanelPageShell({
  pageKey,
  children,
  className,
  scrollClassName,
}: PanelPageShellProps) {
  const sectionClassName = className
    ? `studio-content studio-content--panel ${className}`
    : 'studio-content studio-content--panel';
  const surfaceClassName = scrollClassName
    ? `panel studio-content__panel studio-content__panel--scroll ${scrollClassName}`
    : 'panel studio-content__panel studio-content__panel--scroll';

  return (
    <section className={sectionClassName}>
      <ScrollSurface className={surfaceClassName}>
        <div key={pageKey} className="studio-content__page">
          {children}
        </div>
      </ScrollSurface>
    </section>
  );
}
