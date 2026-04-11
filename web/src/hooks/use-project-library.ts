import { useEffect, useMemo, useState } from 'react';

import type { WorkflowGraph } from '../types';
import { parseWorkflowGraph } from '../lib/graph';
import {
  applyEnvironmentToGraph,
  createNewProjectRecord,
  createProjectSnapshot,
  getActiveEnvironment,
  importProjectsFromText,
  loadProjectLibrary,
  mergeImportedProjects,
  persistProjectLibrary,
  prepareProjectExport,
  renameProjectRecord,
  rollbackProjectToSnapshot,
  type ProjectEnvironment,
  type ProjectEnvironmentDiff,
  type ProjectLibraryState,
  type ProjectRecord,
} from '../lib/projects';

interface UpdateEnvironmentPatch {
  name?: string;
  description?: string;
  diff?: ProjectEnvironmentDiff;
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
  rollbackProject: (projectId: string, snapshotId: string) => ProjectRecord | null;
  setActiveEnvironment: (projectId: string, environmentId: string) => void;
  updateEnvironment: (projectId: string, environmentId: string, patch: UpdateEnvironmentPatch) => void;
  duplicateEnvironment: (projectId: string, environmentId: string) => ProjectEnvironment | null;
  deleteEnvironment: (projectId: string, environmentId: string) => void;
  exportProject: (
    projectId: string,
    nextDraft?: Partial<Pick<ProjectRecord, 'astText' | 'payloadText'>>,
  ) => { fileName: string; text: string } | null;
  getProjectGraphForRuntime: (projectId: string, nextGraph?: WorkflowGraph | null) => WorkflowGraph | null;
}

export interface UseProjectLibraryResult extends ProjectLibraryActions {
  library: ProjectLibraryState;
  projects: ProjectRecord[];
}

function updateProject(
  projects: ProjectRecord[],
  projectId: string,
  updater: (project: ProjectRecord) => ProjectRecord,
): ProjectRecord[] {
  return projects.map((project) => (project.id === projectId ? updater(project) : project));
}

export function useProjectLibrary(): UseProjectLibraryResult {
  const [library, setLibrary] = useState<ProjectLibraryState>(loadProjectLibrary);

  useEffect(() => {
    persistProjectLibrary(library);
  }, [library]);

  const projects = useMemo(
    () => library.projects.slice().sort((left, right) => right.updatedAt.localeCompare(left.updatedAt)),
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

  function createSnapshot(projectId: string, label?: string, description?: string): ProjectRecord | null {
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

  function duplicateEnvironment(projectId: string, environmentId: string): ProjectEnvironment | null {
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
      diff: JSON.parse(JSON.stringify(target.diff)) as ProjectEnvironmentDiff,
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

  function exportProject(
    projectId: string,
    nextDraft?: Partial<Pick<ProjectRecord, 'astText' | 'payloadText'>>,
  ) {
    const target = library.projects.find((project) => project.id === projectId);
    if (!target) {
      return null;
    }

    return prepareProjectExport(nextDraft ? renameProjectRecord(target, nextDraft) : target);
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
    createProject,
    importProjects,
    deleteProject,
    updateProjectDraft,
    saveProject,
    createSnapshot,
    rollbackProject,
    setActiveEnvironment,
    updateEnvironment,
    duplicateEnvironment,
    deleteEnvironment,
    exportProject,
    getProjectGraphForRuntime,
  };
}

function parseProjectGraph(astText: string): WorkflowGraph | null {
  const parsed = parseWorkflowGraph(astText);
  return parsed.graph;
}
