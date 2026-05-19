import type { ExtractionPhase, PhaseInfo } from './types';

export function ProgressIndicator({
  phases,
  currentPhase,
}: {
  phases: PhaseInfo[];
  currentPhase: ExtractionPhase;
}) {
  const currentIdx = phases.findIndex((p) => p.phase === currentPhase);
  const currentLabel = phases[Math.max(currentIdx, 0)]?.label ?? '处理中...';

  return (
    <div className="dm-drawer__progress">
      <div className="dm-drawer__progress-bar">
        {phases.map((p, idx) => (
          <div
            key={p.phase}
            className={`dm-drawer__progress-step${
              idx < currentIdx ? ' is-done' : idx === currentIdx ? ' is-active' : ''
            }`}
          />
        ))}
      </div>
      <div className="dm-drawer__progress-label">
        <span className="dm-drawer__spinner" />
        {currentLabel}
      </div>
    </div>
  );
}
