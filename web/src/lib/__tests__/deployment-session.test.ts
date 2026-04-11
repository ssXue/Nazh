// @vitest-environment jsdom

import { afterEach, describe, expect, it } from 'vitest';

import {
  clearDeploymentSession,
  DEPLOYMENT_SESSION_STORAGE_KEY,
  loadDeploymentSession,
  saveDeploymentSession,
  type PersistedDeploymentSession,
} from '../deployment-session';

afterEach(() => {
  localStorage.clear();
});

function buildSession(): PersistedDeploymentSession {
  return {
    version: 1,
    projectId: 'project-1',
    projectName: '示例工程',
    environmentId: 'env-prod',
    environmentName: '生产',
    deployedAt: '2026-04-11T09:30:00.000Z',
    runtimeAstText: '{"name":"demo"}',
    runtimeConnections: [
      {
        id: 'http-main',
        type: 'http',
        metadata: {
          base_url: 'https://example.com',
        },
      },
    ],
  };
}

describe('deployment session storage', () => {
  it('可以按工作路径保存并读取部署会话', () => {
    const session = buildSession();

    saveDeploymentSession('/Users/demo/Nazh Workspace', session);

    expect(loadDeploymentSession('/Users/demo/Nazh Workspace')).toEqual(session);
  });

  it('无工作路径时使用默认存储键', () => {
    const session = buildSession();

    saveDeploymentSession('', session);

    expect(JSON.parse(localStorage.getItem(DEPLOYMENT_SESSION_STORAGE_KEY) ?? 'null')).toEqual(
      session,
    );
  });

  it('非法会话会被清理', () => {
    localStorage.setItem(DEPLOYMENT_SESSION_STORAGE_KEY, JSON.stringify({ projectId: 'broken' }));

    expect(loadDeploymentSession('')).toBeNull();
    expect(localStorage.getItem(DEPLOYMENT_SESSION_STORAGE_KEY)).toBeNull();
  });

  it('可以清除部署会话', () => {
    saveDeploymentSession('', buildSession());

    clearDeploymentSession('');

    expect(loadDeploymentSession('')).toBeNull();
  });
});
