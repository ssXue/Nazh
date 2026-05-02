// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const invokeMock = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const listenMock = vi.fn(() => Promise.resolve(() => {}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: unknown[]) => listenMock(...args),
}));

import { RuntimeVariablesPanel } from '../RuntimeVariablesPanel';

const mockSnapshot = {
  variables: {
    counter: {
      value: 42,
      variableType: { kind: 'integer' },
      initial: 0,
      updatedAt: '2026-05-03T10:00:00Z',
      updatedBy: 'node-A',
    },
    mode: {
      value: 'auto',
      variableType: { kind: 'string' },
      initial: 'auto',
      updatedAt: '2026-05-03T10:00:00Z',
      updatedBy: null,
    },
  },
};

describe('RuntimeVariablesPanel', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    listenMock.mockClear();
  });

  afterEach(() => {
    cleanup();
  });

  it('无活跃工作流时显示提示文本', () => {
    render(<RuntimeVariablesPanel workflowId={null} />);
    expect(screen.getByText('未选中已部署的工作流')).toBeInTheDocument();
  });

  it('snapshot 返回后渲染变量列表', async () => {
    invokeMock.mockResolvedValue(mockSnapshot);
    render(<RuntimeVariablesPanel workflowId="wf-1" />);

    await waitFor(() => {
      expect(screen.getByText('counter')).toBeInTheDocument();
      expect(screen.getByText('mode')).toBeInTheDocument();
    });
  });

  it('无变量时显示空提示', async () => {
    invokeMock.mockResolvedValue({ variables: {} });
    render(<RuntimeVariablesPanel workflowId="wf-1" />);

    await waitFor(() => {
      expect(screen.getByText('该工作流未声明变量')).toBeInTheDocument();
    });
  });

  it('点击编辑按钮进入编辑模式', async () => {
    invokeMock.mockResolvedValue(mockSnapshot);
    render(<RuntimeVariablesPanel workflowId="wf-1" />);

    await waitFor(() => {
      expect(screen.getByText('counter')).toBeInTheDocument();
    });

    const editButtons = screen.getAllByText('编辑');
    fireEvent.click(editButtons[0]);

    expect(screen.getByDisplayValue('42')).toBeInTheDocument();
  });

  it('编辑提交调用 setWorkflowVariable', async () => {
    invokeMock.mockResolvedValue(mockSnapshot);
    render(<RuntimeVariablesPanel workflowId="wf-1" />);

    await waitFor(() => {
      expect(screen.getByText('counter')).toBeInTheDocument();
    });

    const editButtons = screen.getAllByText('编辑');
    fireEvent.click(editButtons[0]);

    const input = screen.getByDisplayValue('42');
    fireEvent.change(input, { target: { value: '99' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('set_workflow_variable', {
        request: {
          workflowId: 'wf-1',
          name: 'counter',
          value: 99,
        },
      });
    });
  });

  it('点击删除显示确认提示', async () => {
    invokeMock.mockResolvedValue(mockSnapshot);
    render(<RuntimeVariablesPanel workflowId="wf-1" />);

    await waitFor(() => {
      expect(screen.getByText('counter')).toBeInTheDocument();
    });

    const deleteButtons = screen.getAllByText('删除');
    fireEvent.click(deleteButtons[0]);

    expect(screen.getByText('确认删除？')).toBeInTheDocument();
    expect(screen.getByText('是')).toBeInTheDocument();
    expect(screen.getByText('否')).toBeInTheDocument();
  });

  it('确认删除调用 deleteWorkflowVariable', async () => {
    invokeMock.mockResolvedValue(mockSnapshot);
    render(<RuntimeVariablesPanel workflowId="wf-1" />);

    await waitFor(() => {
      expect(screen.getByText('counter')).toBeInTheDocument();
    });

    const deleteButtons = screen.getAllByText('删除');
    fireEvent.click(deleteButtons[0]);

    const confirmYes = screen.getByText('是');
    fireEvent.click(confirmYes);

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('delete_workflow_variable', {
        request: {
          workflowId: 'wf-1',
          name: 'counter',
        },
      });
    });
  });

  it('取消删除恢复原始按钮', async () => {
    invokeMock.mockResolvedValue(mockSnapshot);
    render(<RuntimeVariablesPanel workflowId="wf-1" />);

    await waitFor(() => {
      expect(screen.getByText('counter')).toBeInTheDocument();
    });

    const deleteButtons = screen.getAllByText('删除');
    fireEvent.click(deleteButtons[0]);

    const confirmNo = screen.getByText('否');
    fireEvent.click(confirmNo);

    expect(screen.queryByText('确认删除？')).not.toBeInTheDocument();
    expect(screen.getAllByText('删除').length).toBeGreaterThan(0);
  });

  it('点击重置调用 resetWorkflowVariable', async () => {
    invokeMock.mockResolvedValue(mockSnapshot);
    render(<RuntimeVariablesPanel workflowId="wf-1" />);

    await waitFor(() => {
      expect(screen.getByText('counter')).toBeInTheDocument();
    });

    const resetButtons = screen.getAllByText('重置');
    fireEvent.click(resetButtons[0]);

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('reset_workflow_variable', {
        request: {
          workflowId: 'wf-1',
          name: 'counter',
        },
      });
    });
  });

  it('报告变量数量给父组件', async () => {
    const onCountChange = vi.fn();
    invokeMock.mockResolvedValue(mockSnapshot);
    render(
      <RuntimeVariablesPanel
        workflowId="wf-1"
        onVariableCountChange={onCountChange}
      />,
    );

    await waitFor(() => {
      expect(onCountChange).toHaveBeenCalledWith(2);
    });
  });

  it('snapshot 错误时显示错误消息', async () => {
    invokeMock.mockRejectedValue(new Error('工作流未部署'));
    render(<RuntimeVariablesPanel workflowId="wf-1" />);

    await waitFor(() => {
      expect(screen.getByText('工作流未部署')).toBeInTheDocument();
    });
  });
});
