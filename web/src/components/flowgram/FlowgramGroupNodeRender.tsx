import type { CSSProperties } from 'react';

const FLOWGRAM_GROUP_RENDER_STYLE: CSSProperties = {
  width: '100%',
  height: '100%',
  borderRadius: 18,
  border: '1px dashed rgba(142, 166, 188, 0.34)',
  background: 'rgba(9, 16, 28, 0.3)',
  boxSizing: 'border-box',
};

export function FlowgramGroupNodeRender() {
  return <div style={FLOWGRAM_GROUP_RENDER_STYLE} />;
}
