import { beforeEach, describe, expect, it, vi } from 'vitest';

import {
  buildWorkflowAiNodeGuideText,
  getLocalWorkflowAiNodeCatalog,
  getWorkflowAiAllowedNodeKinds,
} from '../workflow-node-capabilities';

vi.mock('../../lib/tauri', () => ({
  describeNodePins: vi.fn(),
  hasTauriRuntime: vi.fn(() => false),
  listNodeTypes: vi.fn(),
}));

describe('workflow-node-capabilities', () => {
  beforeEach(() => {
    vi.resetModules();
  });

  it('本地能力目录从节点定义自动包含 subgraph，并隐藏桥接节点', () => {
    const catalog = getLocalWorkflowAiNodeCatalog();
    const allowedKinds = getWorkflowAiAllowedNodeKinds(catalog);

    expect(allowedKinds).toContain('subgraph');
    expect(allowedKinds).toContain('code');
    expect(allowedKinds).not.toContain('subgraphInput');
    expect(allowedKinds).not.toContain('subgraphOutput');

    const guide = buildWorkflowAiNodeGuideText(catalog);
    expect(guide).toContain('subgraph');
    expect(guide).toContain('upsert_subgraph');
    expect(guide).toContain('封装子拓扑');
    expect(guide).toContain('配置键');
  });

  it('桌面态合并运行时能力位图和 pin schema', async () => {
    const tauri = await import('../../lib/tauri');
    vi.mocked(tauri.hasTauriRuntime).mockReturnValue(true);
    vi.mocked(tauri.listNodeTypes).mockResolvedValueOnce({
      types: [
        { name: 'timer', capabilities: 1 << 4 },
        { name: 'code', capabilities: 0 },
      ],
    });
    vi.mocked(tauri.describeNodePins).mockResolvedValue({
      inputPins: [
        {
          id: 'in',
          label: 'Input',
          direction: 'input',
          pin_type: { kind: 'json' },
          required: true,
          kind: 'exec',
        },
      ],
      outputPins: [
        {
          id: 'out',
          label: 'Output',
          direction: 'output',
          pin_type: { kind: 'any' },
          required: false,
          kind: 'exec',
        },
      ],
    });

    const { loadWorkflowAiNodeCatalog: loadFreshCatalog } = await import(
      '../workflow-node-capabilities'
    );
    const catalog = await loadFreshCatalog();
    const timer = catalog.nodes.find((node) => node.kind === 'timer');

    expect(timer?.runtimeCapabilities).toContain('触发器');
    expect(timer?.inputPins?.[0]?.id).toBe('in');
    expect(timer?.outputPins?.[0]?.id).toBe('out');
  });
});
