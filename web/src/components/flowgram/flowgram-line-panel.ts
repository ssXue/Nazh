import {
  WorkflowNodePanelService,
  WorkflowNodePanelUtils,
} from '@flowgram.ai/free-node-panel-plugin';
import {
  delay,
  type FreeLayoutPluginContext,
  type onDragLineEndParams,
  WorkflowDragService,
  WorkflowLinesManager,
  WorkflowNodeEntity,
  type WorkflowNodeJSON,
} from '@flowgram.ai/free-layout-editor';

export async function handleFlowgramDragLineEnd(
  ctx: FreeLayoutPluginContext,
  params: onDragLineEndParams,
) {
  const nodePanelService = ctx.get(WorkflowNodePanelService);
  const document = ctx.document;
  const dragService = ctx.get(WorkflowDragService);
  const linesManager = ctx.get(WorkflowLinesManager);
  const { fromPort, toPort, mousePos, line, originLine } = params;

  if (originLine || !line || toPort || !fromPort) {
    return;
  }

  const containerNode = fromPort.node.parent;
  const isVertical = fromPort.location === 'bottom';
  const result = await nodePanelService.singleSelectNodePanel({
    position: isVertical
      ? {
          x: mousePos.x - 165,
          y: mousePos.y + 60,
        }
      : mousePos,
    containerNode,
    panelProps: {
      enableNodePlaceholder: true,
      enableScrollClose: true,
      fromPort,
    },
  });

  if (!result) {
    return;
  }

  const { nodeType, nodeJSON } = result;
  const nodePosition = WorkflowNodePanelUtils.adjustNodePosition({
    nodeType,
    position: mousePos,
    fromPort,
    toPort,
    containerNode,
    document,
    dragService,
  });
  const node: WorkflowNodeEntity = document.createWorkflowNodeByType(
    nodeType,
    nodePosition,
    nodeJSON ?? ({} as WorkflowNodeJSON),
    containerNode?.id,
  );

  await delay(20);

  WorkflowNodePanelUtils.buildLine({
    fromPort,
    node,
    linesManager,
  });
}
