import type { RefObject } from 'react';

import { ConnectionStudio } from '../ConnectionStudio';
import type { UseConnectionLibraryResult } from '../../hooks/use-connection-library';
import type { UseProjectLibraryResult } from '../../hooks/use-project-library';
import type { UseSettingsResult } from '../../hooks/use-settings';
import type { UseWorkflowEngineResult } from '../../hooks/use-workflow-engine';
import type { ProjectEnvironmentDiff, ProjectRecord } from '../../lib/projects';
import { CURRENT_USER_NAME } from '../../lib/projects';
import { ACCENT_PRESET_OPTIONS } from '../../lib/theme';
import type {
  AiConfigUpdate,
  AiConfigView,
  AiProviderDraft,
  AiTestResult,
  ConnectionRecord,
  DeployResponse,
  WorkflowGraph,
  WorkflowWindowStatus,
} from '../../types';
import { AboutPanel } from './AboutPanel';
import { AiConfigPanel } from './AiConfigPanel';
import { BoardWorkspace, type BoardWorkspaceHandle } from './BoardWorkspace';
import { BoardsPanel, type BoardItem } from './BoardsPanel';
import { DashboardPanel } from './DashboardPanel';
import { LogsPanel } from './LogsPanel';
import { PayloadPanel } from './PayloadPanel';
import { PluginPanel } from './PluginPanel';
import { RuntimeManagerPanel } from './RuntimeManagerPanel';
import { SettingsPanel } from './SettingsPanel';
import type { SidebarSection } from './types';

interface StudioContentRouterProps {
  activeBoard: BoardItem | null;
  activeProject: ProjectRecord | null;
  aiActionDisabled: boolean;
  aiActionLoadingCreate: boolean;
  aiActionLoadingEdit: boolean;
  aiActionTitle: string;
  aiConfig: AiConfigView | null;
  aiConfigError: string | null;
  aiConfigLoading: boolean;
  aiTestResult: AiTestResult | null;
  aiTesting: boolean;
  boardItems: BoardItem[];
  canDispatchPayload: boolean;
  connectionLibrary: UseConnectionLibraryResult;
  connectionPreview: ConnectionRecord[];
  connectionUsageById: Map<string, { nodeIds: string[]; projectNames: string[] }>;
  currentBoardDeployInfo: DeployResponse | null;
  engine: UseWorkflowEngineResult;
  flowgramCanvasRef: RefObject<BoardWorkspaceHandle>;
  graph: WorkflowGraph | null;
  graphConnectionCount: number;
  graphEdgeCount: number;
  graphNodeCount: number;
  isTauriRuntime: boolean;
  payloadText: string;
  projectLibrary: UseProjectLibraryResult;
  runtimeModeLabel: string;
  section: SidebarSection;
  settings: UseSettingsResult;
  workflowStatus: WorkflowWindowStatus;
  workflowStatusLabel: string;
  onAfterWorkflowStop: () => void;
  onBackToBoards: () => void;
  onBeforeWorkflowStop: () => void;
  onCreateBoard: () => void;
  onCreateSnapshot: () => void;
  onDeleteBoard: (board: BoardItem) => void;
  onDeleteEnvironment: (environmentId: string) => void;
  onDeleteSnapshot: (snapshotId: string) => void;
  onDispatchPayload: () => Promise<void>;
  onDuplicateEnvironment: (environmentId: string) => void;
  onEnvironmentChange: (environmentId: string) => void;
  onEnvironmentSave: (
    environmentId: string,
    patch: { name: string; description: string; diff: ProjectEnvironmentDiff },
  ) => void;
  onGraphChange: (nextAstText: string) => void;
  onImportBoardFile: (file: File) => void | Promise<void>;
  onOpenAiCreate: () => void;
  onOpenAiEdit: () => void;
  onOpenBoard: (board: BoardItem) => void;
  onPayloadTextChange: (value: string) => void;
  onPersistActiveProject: (projectId: string | null) => Promise<void>;
  onRemovePersistedDeployment: (projectId: string) => Promise<void>;
  onRollbackSnapshot: (snapshotId: string) => void;
  onRuntimeCountChange: (count: number) => void;
  onSectionChange: (section: SidebarSection) => void;
  onStartDeploy: () => Promise<void>;
  onStopDeploy: () => Promise<void>;
  onAiConfigSave: (update: AiConfigUpdate) => Promise<void>;
  onAiProviderTest: (draft: AiProviderDraft) => Promise<void>;
}

function ProjectGate({
  title,
  onNavigateToBoards,
}: {
  title: string;
  onNavigateToBoards: () => void;
}) {
  return (
    <section className="studio-content studio-content--panel">
      <div className="panel studio-content__panel studio-content__panel--scroll studio-gate">
        <div className="studio-gate__copy">
          <h2>{title}</h2>
          <p>先从所有看板进入工程。</p>
        </div>
        <button type="button" onClick={onNavigateToBoards}>
          前往所有看板
        </button>
      </div>
    </section>
  );
}

export function StudioContentRouter({
  activeBoard,
  activeProject,
  aiActionDisabled,
  aiActionLoadingCreate,
  aiActionLoadingEdit,
  aiActionTitle,
  aiConfig,
  aiConfigError,
  aiConfigLoading,
  aiTestResult,
  aiTesting,
  boardItems,
  canDispatchPayload,
  connectionLibrary,
  connectionPreview,
  connectionUsageById,
  currentBoardDeployInfo,
  engine,
  flowgramCanvasRef,
  graph,
  graphConnectionCount,
  graphEdgeCount,
  graphNodeCount,
  isTauriRuntime,
  payloadText,
  projectLibrary,
  runtimeModeLabel,
  section,
  settings,
  workflowStatus,
  workflowStatusLabel,
  onAfterWorkflowStop,
  onBackToBoards,
  onBeforeWorkflowStop,
  onCreateBoard,
  onCreateSnapshot,
  onDeleteBoard,
  onDeleteEnvironment,
  onDeleteSnapshot,
  onDispatchPayload,
  onDuplicateEnvironment,
  onEnvironmentChange,
  onEnvironmentSave,
  onGraphChange,
  onImportBoardFile,
  onOpenAiCreate,
  onOpenAiEdit,
  onOpenBoard,
  onPayloadTextChange,
  onPersistActiveProject,
  onRemovePersistedDeployment,
  onRollbackSnapshot,
  onRuntimeCountChange,
  onSectionChange,
  onStartDeploy,
  onStopDeploy,
  onAiConfigSave,
  onAiProviderTest,
}: StudioContentRouterProps) {
  switch (section) {
    case 'dashboard':
      return (
        <section className="studio-content studio-content--panel">
          <div className="panel studio-content__panel studio-content__panel--scroll">
            <DashboardPanel
              userId={CURRENT_USER_NAME}
              activeBoardName={activeBoard?.name ?? null}
              boardCount={boardItems.length}
              graphNodeCount={graphNodeCount}
              graphEdgeCount={graphEdgeCount}
              graphConnectionCount={graphConnectionCount}
              activeNodeCount={engine.runtimeState.activeNodeIds.length}
              completedNodeCount={engine.runtimeState.completedNodeIds.length}
              failedNodeCount={engine.runtimeState.failedNodeIds.length}
              outputNodeCount={engine.runtimeState.outputNodeIds.length}
              eventCount={engine.eventFeed.length}
              resultCount={engine.results.length}
              statusMessage={engine.statusMessage}
              deployInfo={currentBoardDeployInfo}
              onNavigateToBoards={onBackToBoards}
            />
          </div>
        </section>
      );
    case 'boards':
      if (!activeBoard || !activeProject) {
        return (
          <section className="studio-content studio-content--panel">
            <div className="panel studio-content__panel studio-content__panel--scroll">
              <BoardsPanel
                boards={boardItems}
                onOpenBoard={onOpenBoard}
                onCreateBoard={onCreateBoard}
                onStartAiCreate={onOpenAiCreate}
                onImportBoardFile={onImportBoardFile}
                onDeleteBoard={onDeleteBoard}
                aiActionTitle={aiActionTitle}
                aiActionDisabled={aiActionDisabled}
                aiActionLoading={aiActionLoadingCreate}
              />
            </div>
          </section>
        );
      }

      return (
        <section className="studio-content studio-content--board">
          <BoardWorkspace
            ref={flowgramCanvasRef}
            project={activeProject}
            graph={graph}
            nodeCount={graphNodeCount}
            connectionPreview={connectionPreview}
            themeMode={settings.themeMode}
            isRuntimeDockCollapsed={engine.isRuntimeDockCollapsed}
            flowgramResources={{
              connections: connectionLibrary.connections,
              aiProviders: aiConfig?.providers ?? [],
              activeAiProviderId: aiConfig?.activeProviderId ?? null,
              copilotParams: aiConfig?.copilotParams ?? {},
            }}
            flowgramRuntime={{
              runtimeState: engine.runtimeState,
              workflowStatus,
              canDispatchPayload,
            }}
            flowgramAppearance={{
              accentHex: settings.accentHex,
              nodeCodeColor: settings.accentThemeVariables['--node-code'],
            }}
            flowgramExportTarget={{
              workspacePath: settings.projectWorkspacePath,
              workflowName: activeProject.name,
            }}
            flowgramActions={{
              onRunRequested: onStartDeploy,
              onStopRequested: onStopDeploy,
              onDispatchRequested: onDispatchPayload,
              onGraphChange,
              onError: engine.handleFlowgramError,
              onStatusMessage: engine.setStatusMessage,
            }}
            runtimeDock={{
              eventFeed: engine.eventFeed,
              appErrors: engine.appErrors,
              results: engine.results,
            }}
            onToggleRuntimeDockCollapsed={() =>
              engine.setIsRuntimeDockCollapsed((current) => !current)
            }
            onBack={onBackToBoards}
            onCreateSnapshot={onCreateSnapshot}
            onDeleteSnapshot={onDeleteSnapshot}
            onRollbackSnapshot={onRollbackSnapshot}
            onEnvironmentChange={onEnvironmentChange}
            onEnvironmentSave={onEnvironmentSave}
            onDuplicateEnvironment={onDuplicateEnvironment}
            onDeleteEnvironment={onDeleteEnvironment}
            onOpenAiComposer={onOpenAiEdit}
            aiActionTitle={aiActionTitle}
            aiActionDisabled={aiActionDisabled}
            aiActionLoading={aiActionLoadingEdit}
          />
        </section>
      );
    case 'runtime':
      return (
        <section className="studio-content studio-content--panel">
          <div className="panel studio-content__panel studio-content__panel--scroll">
            <RuntimeManagerPanel
              workspacePath={settings.projectWorkspacePath}
              themeMode={settings.themeMode}
              activeBoardId={activeBoard?.id ?? null}
              onOpenBoard={(boardId) => {
                const targetBoard =
                  boardItems.find((board) => board.id === boardId) ?? {
                    id: boardId,
                    name: boardId,
                    description: '',
                    nodeCount: 0,
                    updatedAt: '',
                    snapshotCount: 0,
                    environmentCount: 0,
                    environmentName: '未选择环境',
                    migrationNote: null,
                  };
                onOpenBoard(targetBoard);
              }}
              onPersistActiveProject={onPersistActiveProject}
              onBeforeWorkflowStop={onBeforeWorkflowStop}
              onAfterWorkflowStop={onAfterWorkflowStop}
              onRemovePersistedDeployment={onRemovePersistedDeployment}
              onStatusMessage={engine.setStatusMessage}
              onRuntimeCountChange={onRuntimeCountChange}
            />
          </div>
        </section>
      );
    case 'connections':
      return (
        <section className="studio-content studio-content--panel">
          <div className="panel studio-content__panel studio-content__panel--scroll panel--connection-card">
            <ConnectionStudio
              connections={connectionLibrary.connections}
              setConnections={connectionLibrary.setConnections}
              usageByConnection={connectionUsageById}
              runtimeConnections={engine.connections}
              isLoading={!connectionLibrary.storage.isReady}
              storageError={connectionLibrary.storage.error}
              onStatusMessage={(msg) => engine.setStatusMessage(msg)}
            />
          </div>
        </section>
      );
    case 'plugins':
      return (
        <section className="studio-content studio-content--panel">
          <div className="panel studio-content__panel studio-content__panel--scroll">
            <PluginPanel isTauriRuntime={isTauriRuntime} />
          </div>
        </section>
      );
    case 'payload':
      if (!activeBoard) {
        return <ProjectGate title="测试载荷" onNavigateToBoards={() => onSectionChange('boards')} />;
      }

      return (
        <section className="studio-content studio-content--panel">
          <div className="panel studio-content__panel studio-content__panel--editor">
            <PayloadPanel
              payloadText={payloadText}
              deployInfo={currentBoardDeployInfo}
              onPayloadTextChange={onPayloadTextChange}
            />
          </div>
        </section>
      );
    case 'logs':
      return (
        <section className="studio-content studio-content--panel">
          <div className="panel studio-content__panel studio-content__panel--scroll">
            <LogsPanel
              eventFeed={engine.eventFeed}
              appErrors={engine.appErrors}
              resultCount={engine.results.length}
              themeMode={settings.themeMode}
              activeBoardName={activeBoard?.name ?? null}
              workflowStatusLabel={workflowStatusLabel}
              workspacePath={settings.projectWorkspacePath}
              activeTraceId={engine.runtimeState.traceId}
            />
          </div>
        </section>
      );
    case 'settings':
      return (
        <section className="studio-content studio-content--panel">
          <div className="panel studio-content__panel studio-content__panel--scroll">
            <SettingsPanel
              isTauriRuntime={isTauriRuntime}
              runtimeModeLabel={runtimeModeLabel}
              workflowStatusLabel={workflowStatusLabel}
              statusMessage={engine.statusMessage}
              themeMode={settings.themeMode}
              onThemeModeChange={settings.setThemeMode}
              accentPreset={settings.accentPreset}
              accentOptions={ACCENT_PRESET_OPTIONS}
              customAccentHex={settings.customAccentHex}
              onAccentPresetChange={settings.setAccentPreset}
              onCustomAccentChange={settings.setCustomAccentHex}
              motionMode={settings.motionMode}
              onMotionModeChange={settings.setMotionMode}
              startupPage={settings.startupPage}
              onStartupPageChange={settings.setStartupPage}
              projectWorkspacePath={settings.projectWorkspacePath}
              projectWorkspaceResolvedPath={projectLibrary.storage.resolvedWorkspacePath}
              projectWorkspaceBoardsDirectoryPath={projectLibrary.storage.boardsDirectoryPath}
              projectWorkspaceUsingDefault={projectLibrary.storage.usingDefaultLocation}
              projectWorkspaceIsSyncing={projectLibrary.storage.isSyncing}
              projectWorkspaceError={projectLibrary.storage.error}
              onProjectWorkspacePathChange={settings.setProjectWorkspacePath}
            />
          </div>
        </section>
      );
    case 'ai':
      return (
        <section className="studio-content studio-content--panel">
          <div className="panel studio-content__panel studio-content__panel--scroll">
            <AiConfigPanel
              isTauriRuntime={isTauriRuntime}
              aiConfig={aiConfig}
              aiConfigLoading={aiConfigLoading}
              aiConfigError={aiConfigError}
              onAiConfigSave={onAiConfigSave}
              onAiProviderTest={onAiProviderTest}
              aiTestResult={aiTestResult}
              aiTesting={aiTesting}
            />
          </div>
        </section>
      );
    case 'about':
      return (
        <section className="studio-content studio-content--panel">
          <div className="panel studio-content__panel studio-content__panel--scroll">
            <AboutPanel />
          </div>
        </section>
      );
  }
}
