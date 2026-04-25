import { useEffect, useState } from 'react';

import { hasTauriRuntime, listRuntimeWorkflows } from '../lib/tauri';

export function useRuntimeWorkflowCount(workspacePath: string) {
  const [runtimeWorkflowCount, setRuntimeWorkflowCount] = useState(0);

  useEffect(() => {
    if (!hasTauriRuntime()) {
      setRuntimeWorkflowCount(0);
      return;
    }

    let cancelled = false;

    const loadRuntimeWorkflowCount = async () => {
      try {
        const workflows = await listRuntimeWorkflows();
        if (!cancelled) {
          setRuntimeWorkflowCount(workflows.length);
        }
      } catch {
        if (!cancelled) {
          setRuntimeWorkflowCount(0);
        }
      }
    };

    void loadRuntimeWorkflowCount();
    const timer = window.setInterval(() => {
      void loadRuntimeWorkflowCount();
    }, 2500);

    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [workspacePath]);

  return {
    runtimeWorkflowCount,
    setRuntimeWorkflowCount,
  };
}
