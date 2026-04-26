import { forwardRef } from 'react';

import { FlowgramCanvas, type FlowgramCanvasHandle } from '../FlowgramCanvas';
import type {
  FlowgramCanvasActions,
  FlowgramCanvasAppearance,
  FlowgramCanvasExportTarget,
  FlowgramCanvasResources,
  FlowgramCanvasRuntime,
} from '../FlowgramCanvas';
import type { ProjectEnvironmentDiff, ProjectRecord } from '../../lib/projects';
import type { ConnectionRecord, WorkflowGraph } from '../../types';
import type { RuntimeDockProps, ThemeMode } from './types';
import { ProjectWorkspaceHeader } from './ProjectWorkspaceHeader';
import { RuntimeDock } from './RuntimeDock';

export type BoardWorkspaceHandle = FlowgramCanvasHandle;

interface BoardWorkspaceProps {
  project: ProjectRecord;
  graph: WorkflowGraph | null;
  nodeCount: number;
  connectionPreview: ConnectionRecord[];
  themeMode: ThemeMode;
  isRuntimeDockCollapsed: boolean;
  flowgramResources: FlowgramCanvasResources;
  flowgramRuntime: FlowgramCanvasRuntime;
  flowgramAppearance: FlowgramCanvasAppearance;
  flowgramExportTarget: FlowgramCanvasExportTarget;
  flowgramActions: FlowgramCanvasActions;
  runtimeDock: Pick<RuntimeDockProps, 'eventFeed' | 'appErrors' | 'results'>;
  onToggleRuntimeDockCollapsed: () => void;
  onBack: () => void;
  onCreateSnapshot: () => void;
  onDeleteSnapshot: (snapshotId: string) => void;
  onRollbackSnapshot: (snapshotId: string) => void;
  onEnvironmentChange: (environmentId: string) => void;
  onEnvironmentSave: (
    environmentId: string,
    patch: { name: string; description: string; diff: ProjectEnvironmentDiff },
  ) => void;
  onDuplicateEnvironment: (environmentId: string) => void;
  onDeleteEnvironment: (environmentId: string) => void;
  onOpenAiComposer: () => void;
  aiActionTitle: string;
  aiActionDisabled?: boolean;
  aiActionLoading?: boolean;
}

export const BoardWorkspace = forwardRef<BoardWorkspaceHandle, BoardWorkspaceProps>(
  function BoardWorkspace(
    {
      project,
      graph,
      nodeCount,
      connectionPreview,
      themeMode,
      isRuntimeDockCollapsed,
      flowgramResources,
      flowgramRuntime,
      flowgramAppearance,
      flowgramExportTarget,
      flowgramActions,
      runtimeDock,
      onToggleRuntimeDockCollapsed,
      onBack,
      onCreateSnapshot,
      onDeleteSnapshot,
      onRollbackSnapshot,
      onEnvironmentChange,
      onEnvironmentSave,
      onDuplicateEnvironment,
      onDeleteEnvironment,
      onOpenAiComposer,
      aiActionTitle,
      aiActionDisabled = false,
      aiActionLoading = false,
    },
    ref,
  ) {
    return (
      <div
        className={`studio-board-workspace ${isRuntimeDockCollapsed ? 'is-runtime-collapsed' : ''}`}
      >
        <div className="studio-board-workspace__stage">
          <ProjectWorkspaceHeader
            project={project}
            nodeCount={nodeCount}
            onBack={onBack}
            onCreateSnapshot={onCreateSnapshot}
            onDeleteSnapshot={onDeleteSnapshot}
            onRollbackSnapshot={onRollbackSnapshot}
            onEnvironmentChange={onEnvironmentChange}
            onEnvironmentSave={onEnvironmentSave}
            onDuplicateEnvironment={onDuplicateEnvironment}
            onDeleteEnvironment={onDeleteEnvironment}
            onOpenAiComposer={onOpenAiComposer}
            isRuntimeDockCollapsed={isRuntimeDockCollapsed}
            onToggleRuntimeDockCollapsed={onToggleRuntimeDockCollapsed}
            aiActionTitle={aiActionTitle}
            aiActionDisabled={aiActionDisabled}
            aiActionLoading={aiActionLoading}
          />

          <FlowgramCanvas
            ref={ref}
            graph={graph}
            resources={flowgramResources}
            runtime={flowgramRuntime}
            appearance={flowgramAppearance}
            exportTarget={flowgramExportTarget}
            actions={flowgramActions}
          />
        </div>

        <RuntimeDock
          eventFeed={runtimeDock.eventFeed}
          appErrors={runtimeDock.appErrors}
          results={runtimeDock.results}
          connectionPreview={connectionPreview}
          themeMode={themeMode}
          isCollapsed={isRuntimeDockCollapsed}
          onToggleCollapsed={onToggleRuntimeDockCollapsed}
        />
      </div>
    );
  },
);
