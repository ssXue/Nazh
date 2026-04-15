import { describe, expect, it } from 'vitest';

import type { PersistedDeploymentSessionState } from '../deployment-session';
import { mergePersistedDeploymentSessionStates } from '../persisted-deployment-state';

function buildState(
  projectId: string,
  deployedAt: string,
): PersistedDeploymentSessionState {
  return {
    version: 3,
    activeProjectId: projectId,
    sessions: [
      {
        version: 1,
        projectId,
        projectName: projectId,
        environmentId: 'env-prod',
        environmentName: '生产',
        deployedAt,
        runtimeAstText: `{"name":"${projectId}"}`,
        runtimeConnections: [],
      },
    ],
  };
}

describe('persisted deployment session state', () => {
  it('合并文件态和本地降级态时，本地主控工程优先', () => {
    const fileState = buildState('board-file', '2026-04-15T01:00:00.000Z');
    const localFallbackState = buildState('board-local', '2026-04-15T02:00:00.000Z');

    expect(
      mergePersistedDeploymentSessionStates(fileState, localFallbackState),
    ).toMatchObject({
      activeProjectId: 'board-local',
      sessions: [
        expect.objectContaining({ projectId: 'board-local' }),
        expect.objectContaining({ projectId: 'board-file' }),
      ],
    });
  });
});
