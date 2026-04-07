import type { CSSProperties } from 'react';

const FLOWGRAM_GROUP_RENDER_STYLE: CSSProperties = {
  width: '100%',
  height: '100%',
  borderRadius: 18,
  border: '1px dashed var(--line-dashed)',
  background: 'linear-gradient(180deg, var(--accent-soft), var(--surface-muted))',
  boxSizing: 'border-box',
};

export function FlowgramGroupNodeRender() {
  return <div style={FLOWGRAM_GROUP_RENDER_STYLE} />;
}
