import { useEffect, useMemo, useRef, useState } from 'react';

import type { WorkflowGraph } from '../types';
import { parseWorkflowGraph } from '../lib/graph';
import {
  applyEnvironmentToGraph,
  createNewProjectRecord,
  createProjectSnapshot,
  deleteProjectSnapshot,
  getActiveEnvironment,
  importProjectsFromText,
  loadProjectLibrary,
  mergeImportedProjects,
  parseProjectBoardFiles,
  parseProjectLibraryText,
  persistProjectLibrary,
  renameProjectRecord,
  rollbackProjectToSnapshot,
  serializeProjectLibraryToBoardFiles,
  type ProjectEnvironment,
  type ProjectEnvironmentDiff,
  type ProjectLibraryState,
  type ProjectRecord,
} from '../lib/projects';
import {
  hasTauriRuntime,
  loadProjectBoardFiles,
  saveProjectBoardFiles,
} from '../lib/tauri';

interface UpdateEnvironmentPatch {
  name?: string;
  description?: string;
  diff?: ProjectEnvironmentDiff;
}

export interface ProjectLibraryStorageState {
  isReady: boolean;
  isSyncing: boolean;
  resolvedWorkspacePath: string | null;
  boardsDirectoryPath: string | null;
  usingDefaultLocation: boolean;
  error: string | null;
}

export interface ProjectLibraryActions {
  createProject: (name?: string, description?: string) => ProjectRecord;
  importProjects: (sourceText: string) => { importedProjects: ProjectRecord[]; migrationNotes: string[] };
  deleteProject: (projectId: string) => ProjectRecord | null;
  updateProjectDraft: (
    projectId: string,
    nextDraft: Partial<Pick<ProjectRecord, 'astText' | 'payloadText' | 'name' | 'description'>>,
  ) => void;
  saveProject: (
    projectId: string,
    nextDraft?: Partial<Pick<ProjectRecord, 'astText' | 'payloadText'>>,
  ) => ProjectRecord | null;
  createSnapshot: (projectId: string, label?: string, description?: string) => ProjectRecord | null;
  deleteSnapshot: (projectId: string, snapshotId: string) => ProjectRecord | null;
  rollbackProject: (projectId: string, snapshotId: string) => ProjectRecord | null;
  setActiveEnvironment: (projectId: string, environmentId: string) => void;
  updateEnvironment: (projectId: string, environmentId: string, patch: UpdateEnvironmentPatch) => void;
  duplicateEnvironment: (projectId: string, environmentId: string) => ProjectEnvironment | null;
  deleteEnvironment: (projectId: string, environmentId: string) => void;
  getProjectGraphForRuntime: (projectId: string, nextGraph?: WorkflowGraph | null) => WorkflowGraph | null;
}

export interface UseProjectLibraryResult extends ProjectLibraryActions {
  library: ProjectLibraryState;
  projects: ProjectRecord[];
  storage: ProjectLibraryStorageState;
}

function updateProject(
  projects: ProjectRecord[],
  projectId: string,
  updater: (project: ProjectRecord) => ProjectRecord,
): ProjectRecord[] {
  return projects.map((project) => (project.id === projectId ? updater(project) : project));
}

function cloneEnvironmentDiff(diff: ProjectEnvironmentDiff): ProjectEnvironmentDiff {
  return JSON.parse(JSON.stringify(diff)) as ProjectEnvironmentDiff;
}

function describeStorageError(error: unknown): string {
  if (typeof error === 'string' && error.trim()) {
    return error;
  }

  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }

  return '看板文件同步失败。';
}

export function useProjectLibrary(workspacePath = ''): UseProjectLibraryResult {
  const desktopStorageEnabled = hasTauriRuntime();
  const normalizedWorkspacePath = workspacePath.trim();
  const [library, setLibrary] = useState<ProjectLibraryState>(loadProjectLibrary);
  const [storage, setStorage] = useState<ProjectLibraryStorageState>(() => ({
    isReady: !desktopStorageEnabled,
    isSyncing: false,
    resolvedWorkspacePath: null,
    boardsDirectoryPath: null,
    usingDefaultLocation: normalizedWorkspacePath.length === 0,
    error: null,
  }));
  const [hydratedWorkspacePath, setHydratedWorkspacePath] = useState<string | null>(
    desktopStorageEnabled ? null : normalizedWorkspacePath,
  );
  const latestLibraryRef = useRef(library);

  useEffect(() => {
    latestLibraryRef.current = library;
  }, [library]);

  useEffect(() => {
    persistProjectLibrary(library);
  }, [library]);

  useEffect(() => {
    if (!desktopStorageEnabled) {
      setStorage({
        isReady: true,
        isSyncing: false,
        resolvedWorkspacePath: null,
        boardsDirectoryPath: null,
        usingDefaultLocation: true,
        error: null,
      });
      setHydratedWorkspacePath(normalizedWorkspacePath);
      return;
    }

    let cancelled = false;
    const fallbackLibrary = latestLibraryRef.current;

    setStorage((current) => ({
      ...current,
      isReady: false,
      isSyncing: true,
      usingDefaultLocation: normalizedWorkspacePath.length === 0,
      error: null,
    }));

    void loadProjectBoardFiles(normalizedWorkspacePath)
      .then(async (result) => {
        let nextLibrary = fallbackLibrary;
        let nextStorage = result.storage;

        if (result.boardFiles.length > 0) {
          nextLibrary = parseProjectBoardFiles(result.boardFiles);
        } else if (result.legacyLibraryText) {
          nextLibrary = parseProjectLibraryText(result.legacyLibraryText);
        } else {
          nextStorage = await saveProjectBoardFiles(
            normalizedWorkspacePath,
            serializeProjectLibraryToBoardFiles(fallbackLibrary),
          );
        }

        if (cancelled) {
          return;
        }

        setLibrary(nextLibrary);
        setStorage({
          isReady: true,
          isSyncing: false,
          resolvedWorkspacePath: nextStorage.workspacePath,
          boardsDirectoryPath: nextStorage.boardsDirectoryPath,
          usingDefaultLocation: nextStorage.usingDefaultLocation,
          error: null,
        });
        setHydratedWorkspacePath(normalizedWorkspacePath);
      })
      .catch((error) => {
        if (cancelled) {
          return;
        }

        setStorage((current) => ({
          ...current,
          isReady: true,
          isSyncing: false,
          error: describeStorageError(error),
        }));
      });

    return () => {
      cancelled = true;
    };
  }, [desktopStorageEnabled, normalizedWorkspacePath]);

  useEffect(() => {
    if (
      !desktopStorageEnabled ||
      !storage.isReady ||
      hydratedWorkspacePath !== normalizedWorkspacePath
    ) {
      return;
    }

    let cancelled = false;

    setStorage((current) => ({
      ...current,
      isSyncing: true,
      error: null,
    }));

    void saveProjectBoardFiles(normalizedWorkspacePath, serializeProjectLibraryToBoardFiles(library))
      .then((result) => {
        if (cancelled) {
          return;
        }

        setStorage({
          isReady: true,
          isSyncing: false,
          resolvedWorkspacePath: result.workspacePath,
          boardsDirectoryPath: result.boardsDirectoryPath,
          usingDefaultLocation: result.usingDefaultLocation,
          error: null,
        });
      })
      .catch((error) => {
        if (cancelled) {
          return;
        }

        setStorage((current) => ({
          ...current,
          isSyncing: false,
          error: describeStorageError(error),
        }));
      });

    return () => {
      cancelled = true;
    };
  }, [desktopStorageEnabled, hydratedWorkspacePath, library, normalizedWorkspacePath, storage.isReady]);

  const projects = useMemo(
    () =>
      library.projects
        .slice()
        .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt)),
    [library.projects],
  );

  function createProject(name?: string, description?: string): ProjectRecord {
    const baseProject = createNewProjectRecord(
      name?.trim() || `未命名工程 ${library.projects.length + 1}`,
      description,
    );
    const uniqueId = new Set(library.projects.map((project) => project.id)).has(baseProject.id)
      ? `${baseProject.id}-${library.projects.length + 1}`
      : baseProject.id;
    const nextProject = { ...baseProject, id: uniqueId };

    setLibrary((current) => ({
      ...current,
      projects: [nextProject, ...current.projects],
    }));

    return nextProject;
  }

  function importProjects(sourceText: string) {
    const result = importProjectsFromText(sourceText);
    let nextProjects = library.projects.slice();
    const importedProjects = result.importedProjects.map((project, index) => {
      let nextId = project.id;
      if (nextProjects.some((item) => item.id === nextId)) {
        nextId = `${project.id}-${nextProjects.length + index + 1}`;
      }

      const nextProject = {
        ...project,
        id: nextId,
        updatedAt: new Date().toISOString(),
      };

      nextProjects = mergeImportedProjects(nextProjects, [nextProject]);
      return nextProject;
    });

    setLibrary((current) => ({
      ...current,
      projects: nextProjects,
    }));

    return {
      importedProjects,
      migrationNotes: result.migrationNotes,
    };
  }

  function deleteProject(projectId: string): ProjectRecord | null {
    const target = library.projects.find((project) => project.id === projectId);
    if (!target) {
      return null;
    }

    setLibrary((current) => ({
      ...current,
      projects: current.projects.filter((project) => project.id !== projectId),
    }));

    return target;
  }

  function updateProjectDraft(
    projectId: string,
    nextDraft: Partial<Pick<ProjectRecord, 'astText' | 'payloadText' | 'name' | 'description'>>,
  ) {
    setLibrary((current) => ({
      ...current,
      projects: updateProject(current.projects, projectId, (project) =>
        renameProjectRecord(project, nextDraft),
      ),
    }));
  }

  function saveProject(
    projectId: string,
    nextDraft?: Partial<Pick<ProjectRecord, 'astText' | 'payloadText'>>,
  ): ProjectRecord | null {
    const target = library.projects.find((project) => project.id === projectId);
    if (!target) {
      return null;
    }

    const nextProject = renameProjectRecord(target, nextDraft ?? {});
    setLibrary((current) => ({
      ...current,
      projects: updateProject(current.projects, projectId, () => nextProject),
    }));
    return nextProject;
  }

  function createSnapshot(
    projectId: string,
    label?: string,
    description?: string,
  ): ProjectRecord | null {
    const target = library.projects.find((project) => project.id === projectId);
    if (!target) {
      return null;
    }

    const nextProject = createProjectSnapshot(target, label, description);
    setLibrary((current) => ({
      ...current,
      projects: updateProject(current.projects, projectId, () => nextProject),
    }));
    return nextProject;
  }

  function deleteSnapshot(projectId: string, snapshotId: string): ProjectRecord | null {
    const target = library.projects.find((project) => project.id === projectId);
    if (!target) {
      return null;
    }

    const nextProject = deleteProjectSnapshot(target, snapshotId);
    setLibrary((current) => ({
      ...current,
      projects: updateProject(current.projects, projectId, () => nextProject),
    }));
    return nextProject;
  }

  function rollbackProject(projectId: string, snapshotId: string): ProjectRecord | null {
    const target = library.projects.find((project) => project.id === projectId);
    if (!target) {
      return null;
    }

    const nextProject = rollbackProjectToSnapshot(target, snapshotId);
    setLibrary((current) => ({
      ...current,
      projects: updateProject(current.projects, projectId, () => nextProject),
    }));
    return nextProject;
  }

  function setActiveEnvironment(projectId: string, environmentId: string) {
    setLibrary((current) => ({
      ...current,
      projects: updateProject(current.projects, projectId, (project) => ({
        ...project,
        activeEnvironmentId: environmentId,
        updatedAt: new Date().toISOString(),
      })),
    }));
  }

  function updateEnvironment(
    projectId: string,
    environmentId: string,
    patch: UpdateEnvironmentPatch,
  ) {
    setLibrary((current) => ({
      ...current,
      projects: updateProject(current.projects, projectId, (project) => ({
        ...project,
        updatedAt: new Date().toISOString(),
        environments: project.environments.map((environment) =>
          environment.id === environmentId
            ? {
                ...environment,
                ...patch,
                updatedAt: new Date().toISOString(),
                diff: patch.diff ? patch.diff : environment.diff,
              }
            : environment,
        ),
      })),
    }));
  }

  function duplicateEnvironment(
    projectId: string,
    environmentId: string,
  ): ProjectEnvironment | null {
    const project = library.projects.find((item) => item.id === projectId);
    const target = project?.environments.find((environment) => environment.id === environmentId);
    if (!project || !target) {
      return null;
    }

    const duplicatedEnvironment: ProjectEnvironment = {
      ...target,
      id: `${target.id}-copy-${project.environments.length + 1}`,
      name: `${target.name} 副本`,
      updatedAt: new Date().toISOString(),
      diff: cloneEnvironmentDiff(target.diff),
    };

    setLibrary((current) => ({
      ...current,
      projects: updateProject(current.projects, projectId, (currentProject) => ({
        ...currentProject,
        updatedAt: new Date().toISOString(),
        activeEnvironmentId: duplicatedEnvironment.id,
        environments: [...currentProject.environments, duplicatedEnvironment],
      })),
    }));

    return duplicatedEnvironment;
  }

  function deleteEnvironment(projectId: string, environmentId: string) {
    setLibrary((current) => ({
      ...current,
      projects: updateProject(current.projects, projectId, (project) => {
        if (project.environments.length <= 1) {
          return project;
        }

        const nextEnvironments = project.environments.filter(
          (environment) => environment.id !== environmentId,
        );
        const nextActiveEnvironmentId =
          project.activeEnvironmentId === environmentId
            ? nextEnvironments[0]?.id ?? ''
            : project.activeEnvironmentId;

        return {
          ...project,
          updatedAt: new Date().toISOString(),
          activeEnvironmentId: nextActiveEnvironmentId,
          environments: nextEnvironments,
        };
      }),
    }));
  }

  function getProjectGraphForRuntime(projectId: string, nextGraph?: WorkflowGraph | null) {
    const target = library.projects.find((project) => project.id === projectId);
    if (!target) {
      return null;
    }

    const currentGraph = nextGraph ?? parseProjectGraph(target.astText);
    if (!currentGraph) {
      return null;
    }

    return applyEnvironmentToGraph(currentGraph, getActiveEnvironment(target));
  }

  return {
    library,
    projects,
    storage,
    createProject,
    importProjects,
    deleteProject,
    updateProjectDraft,
    saveProject,
    createSnapshot,
    deleteSnapshot,
    rollbackProject,
    setActiveEnvironment,
    updateEnvironment,
    duplicateEnvironment,
    deleteEnvironment,
    getProjectGraphForRuntime,
  };
}

function parseProjectGraph(astText: string): WorkflowGraph | null {
  const parsed = parseWorkflowGraph(astText);
  return parsed.graph;
}
