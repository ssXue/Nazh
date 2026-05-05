import { StrictMode, type ReactNode, type RefObject } from 'react';

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
import { clearObservability, hasTauriRuntime } from '../../lib/tauri';
import { AboutPanel } from './AboutPanel';
import { AiConfigPanel } from './AiConfigPanel';
import { BoardWorkspace, type BoardWorkspaceHandle } from './BoardWorkspace';
import { BoardsPanel, type BoardItem } from './BoardsPanel';
import { DashboardPanel } from './DashboardPanel';
import { DeviceModelingPanel } from './DeviceModelingPanel';
import { LogsPanel } from './LogsPanel';
import { PluginPanel } from './PluginPanel';
import { RuntimeManagerPanel } from './RuntimeManagerPanel';
import { ScrollSurface } from './ScrollSurface';
import { SettingsPanel } from './SettingsPanel';
import type { SidebarSection } from './types';

interface StudioContentRouterProps {
  activeBoard: BoardItem | null;
  activeProject: ProjectRecord | null;
  aiActionDisabled: boolean;
  aiActionLoadingEdit: boolean;
  aiActionTitle: string;
  aiConfig: AiConfigView | null;
  aiConfigError: string | null;
  aiConfigLoading: boolean;
  aiTestResult: AiTestResult | null;
  aiTesting: boolean;
  boardItems: BoardItem[];
  canTestRun: boolean;
  connectionLibrary: UseConnectionLibraryResult;
  connectionPreview: ConnectionRecord[];
  connectionUsageById: Map<string, { nodeIds: string[]; projectNames: string[] }>;
  currentBoardDeployInfo: DeployResponse | null;
  engine: UseWorkflowEngineResult;
  flowgramCanvasRef: RefObject<BoardWorkspaceHandle | null>;
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
  onTestRun: () => Promise<void>;
  onDuplicateEnvironment: (environmentId: string) => void;
  onEnvironmentChange: (environmentId: string) => void;
  onEnvironmentSave: (
    environmentId: string,
    patch: { name: string; description: string; diff: ProjectEnvironmentDiff },
  ) => void;
  onGraphChange: (nextAstText: string) => void;
  onImportBoardFile: (file: File) => void | Promise<void>;
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
      <ScrollSurface className="panel studio-content__panel studio-content__panel--scroll studio-gate">
        <div className="studio-gate__copy">
          <h2>{title}</h2>
          <p>先从所有看板进入工程。</p>
        </div>
        <button type="button" onClick={onNavigateToBoards}>
          前往所有看板
        </button>
      </ScrollSurface>
    </section>
  );
}

function StrictStudioPanel({ children }: { children: ReactNode }) {
  return <StrictMode>{children}</StrictMode>;
}

export function StudioContentRouter({
  activeBoard,
  activeProject,
  aiActionDisabled,
  aiActionLoadingEdit,
  aiActionTitle,
  aiConfig,
  aiConfigError,
  aiConfigLoading,
  aiTestResult,
  aiTesting,
  boardItems,
  canTestRun,
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
  onTestRun,
  onDuplicateEnvironment,
  onEnvironmentChange,
  onEnvironmentSave,
  onGraphChange,
  onImportBoardFile,
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
        <StrictStudioPanel>
          <section className="studio-content studio-content--panel">
            <ScrollSurface className="panel studio-content__panel studio-content__panel--scroll">
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
                workflowStatus={workflowStatus}
                traceId={engine.runtimeState.traceId}
                lastEventType={engine.runtimeState.lastEventType}
                lastNodeId={engine.runtimeState.lastNodeId}
                lastUpdatedAt={engine.runtimeState.lastUpdatedAt}
                connections={engine.connections}
                eventFeed={engine.eventFeed}
                onNavigateToBoards={onBackToBoards}
              />
            </ScrollSurface>
          </section>
        </StrictStudioPanel>
      );
    case 'boards':
      if (!activeBoard || !activeProject) {
        return (
          <StrictStudioPanel>
            <section className="studio-content studio-content--panel">
              <ScrollSurface className="panel studio-content__panel studio-content__panel--scroll">
                <BoardsPanel
                  boards={boardItems}
                  onOpenBoard={onOpenBoard}
                  onCreateBoard={onCreateBoard}
                  onImportBoardFile={onImportBoardFile}
                  onDeleteBoard={onDeleteBoard}
                />
              </ScrollSurface>
            </section>
          </StrictStudioPanel>
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
              canTestRun,
            }}
            flowgramAppearance={{
              accentHex: settings.accentHex,
              themeMode: settings.themeMode,
              nodeCodeColor: settings.accentThemeVariables['--node-code'],
            }}
            flowgramExportTarget={{
              workspacePath: settings.projectWorkspacePath,
              workflowName: activeProject.name,
            }}
            flowgramActions={{
              onRunRequested: onStartDeploy,
              onStopRequested: onStopDeploy,
              onTestRunRequested: onTestRun,
              onGraphChange,
              onError: engine.handleFlowgramError,
              onStatusMessage: engine.setStatusMessage,
            }}
            runtimeDock={{
              eventFeed: engine.eventFeed,
              appErrors: engine.appErrors,
              results: engine.results,
              activeWorkflowId: currentBoardDeployInfo?.workflowId ?? null,
              payloadText,
              deployInfo: currentBoardDeployInfo,
              onPayloadTextChange,
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
        <StrictStudioPanel>
          <section className="studio-content studio-content--panel">
            <ScrollSurface className="panel studio-content__panel studio-content__panel--scroll">
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
            </ScrollSurface>
          </section>
        </StrictStudioPanel>
      );
    case 'connections':
      return (
        <StrictStudioPanel>
          <section className="studio-content studio-content--panel">
            <ScrollSurface className="panel studio-content__panel studio-content__panel--scroll panel--connection-card">
              <ConnectionStudio
                connections={connectionLibrary.connections}
                setConnections={connectionLibrary.setConnections}
                usageByConnection={connectionUsageById}
                runtimeConnections={engine.connections}
                isLoading={!connectionLibrary.storage.isReady}
                storageError={connectionLibrary.storage.error}
                onStatusMessage={(msg) => engine.setStatusMessage(msg)}
              />
            </ScrollSurface>
          </section>
        </StrictStudioPanel>
      );
    case 'devices':
      return (
        <StrictStudioPanel>
          <section className="studio-content studio-content--panel">
            <ScrollSurface className="panel studio-content__panel studio-content__panel--scroll">
              <DeviceModelingPanel
                isTauriRuntime={isTauriRuntime}
                onStatusMessage={engine.setStatusMessage}
              />
            </ScrollSurface>
          </section>
        </StrictStudioPanel>
      );
    case 'plugins':
      return (
        <StrictStudioPanel>
          <section className="studio-content studio-content--panel">
            <ScrollSurface className="panel studio-content__panel studio-content__panel--scroll">
              <PluginPanel isTauriRuntime={isTauriRuntime} />
            </ScrollSurface>
          </section>
        </StrictStudioPanel>
      );
    case 'logs':
      return (
        <StrictStudioPanel>
          <section className="studio-content studio-content--panel">
            <ScrollSurface className="panel studio-content__panel studio-content__panel--scroll">
              <LogsPanel
                eventFeed={engine.eventFeed}
                appErrors={engine.appErrors}
                resultCount={engine.results.length}
                themeMode={settings.themeMode}
                activeBoardName={activeBoard?.name ?? null}
                workspacePath={settings.projectWorkspacePath}
                activeTraceId={engine.runtimeState.traceId}
                onClearLogs={() => {
                  engine.clearLogs();
                  if (hasTauriRuntime()) {
                    void clearObservability(settings.projectWorkspacePath);
                  }
                }}
              />
            </ScrollSurface>
          </section>
        </StrictStudioPanel>
      );
    case 'settings':
      return (
        <StrictStudioPanel>
          <section className="studio-content studio-content--panel">
            <ScrollSurface className="panel studio-content__panel studio-content__panel--scroll">
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
            </ScrollSurface>
          </section>
        </StrictStudioPanel>
      );
    case 'ai':
      return (
        <StrictStudioPanel>
          <section className="studio-content studio-content--panel">
            <ScrollSurface className="panel studio-content__panel studio-content__panel--scroll">
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
            </ScrollSurface>
          </section>
        </StrictStudioPanel>
      );
    case 'about':
      return (
        <StrictStudioPanel>
          <section className="studio-content studio-content--panel">
            <ScrollSurface className="panel studio-content__panel studio-content__panel--scroll">
              <AboutPanel />
            </ScrollSurface>
          </section>
        </StrictStudioPanel>
      );
  }
}
