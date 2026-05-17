import type {
  FreeLayoutPluginContext,
  WorkflowLinesManager,
} from '@flowgram.ai/free-layout-editor';
import { useCallback } from 'react';

import {
  type ConnectionRejection,
  checkConnection,
  formatRejection,
} from '../../lib/pin-validator';

function applyConnectionRejectionFeedback(
  toPort: { hasError?: boolean },
  rejection: ConnectionRejection,
): void {
  toPort.hasError = true;
  window.setTimeout(() => {
    toPort.hasError = false;
  }, 1500);
  console.warn(`[pin-validator] ${formatRejection(rejection)}`);
}

export function useFlowgramConnectionValidation() {
  return useCallback(
    (
      _ctx: FreeLayoutPluginContext,
      fromPort: { node: { id: string }; portID: string | number },
      toPort: { node: { id: string }; portID: string | number; hasError?: boolean },
      _lines: WorkflowLinesManager,
      silent?: boolean,
    ): boolean => {
      const result = checkConnection(
        fromPort.node.id,
        fromPort.portID,
        toPort.node.id,
        toPort.portID,
      );

      if (!result.allow && result.rejection && !silent) {
        applyConnectionRejectionFeedback(toPort, result.rejection);
      }

      return result.allow;
    },
    [],
  );
}
