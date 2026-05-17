import type { WorkflowJSON as FlowgramWorkflowJSON } from '@flowgram.ai/free-layout-editor';

import type {
  ConnectionDefinition,
  JsonValue,
  WorkflowGraph,
  WorkflowNodeDefinition,
} from '../types';

export const CURRENT_USER_NAME = 'ssxue';
export const PROJECT_LIBRARY_STORAGE_KEY = 'nazh.project-library';
export const PROJECT_PACKAGE_KIND = 'nazh.project';
export const PROJECT_LIBRARY_KIND = 'nazh.project-library';
export const PROJECT_BOARD_KIND = 'nazh.board';
export const PROJECT_SCHEMA_VERSION = 2;
export const MAX_PROJECT_SNAPSHOTS = 20;
export const PROJECT_BOARD_FILE_SUFFIX = '.nazh-board.json';

export interface ProjectEnvironmentDiff {
  connections?: Record<string, JsonValue>;
  nodeConfigs?: Record<string, JsonValue>;
}

export interface ProjectEnvironment {
  id: string;
  name: string;
  description: string;
  updatedAt: string;
  diff: ProjectEnvironmentDiff;
}

export type ProjectSnapshotReason =
  | 'seed'
  | 'manual'
  | 'import'
  | 'migration'
  | 'rollback';

export interface ProjectSnapshot {
  id: string;
  label: string;
  description: string;
  createdAt: string;
  reason: ProjectSnapshotReason;
  astText: string;
  payloadText: string;
  activeEnvironmentId: string;
  environments: ProjectEnvironment[];
}

export interface ProjectRecord {
  id: string;
  name: string;
  description: string;
  createdAt: string;
  updatedAt: string;
  astText: string;
  payloadText: string;
  activeEnvironmentId: string;
  environments: ProjectEnvironment[];
  snapshots: ProjectSnapshot[];
  migrationNotes: string[];
}

export interface ProjectLibraryState {
  kind: typeof PROJECT_LIBRARY_KIND;
  schemaVersion: typeof PROJECT_SCHEMA_VERSION;
  projects: ProjectRecord[];
}

export interface ProjectPackage {
  kind: typeof PROJECT_PACKAGE_KIND;
  schemaVersion: typeof PROJECT_SCHEMA_VERSION;
  exportedAt: string;
  project: ProjectRecord;
}

export interface ProjectBoardFileText {
  fileName: string;
  text: string;
}

export interface ProjectBoardSnapshotFile {
  id: string;
  label: string;
  description: string;
  createdAt: string;
  reason: ProjectSnapshotReason;
  nodes: FlowgramWorkflowJSON['nodes'];
  edges: FlowgramWorkflowJSON['edges'];
  payloadText: string;
  activeEnvironmentId: string;
  environments: ProjectEnvironment[];
}

export interface ProjectBoardFile {
  kind: typeof PROJECT_BOARD_KIND;
  schemaVersion: typeof PROJECT_SCHEMA_VERSION;
  id: string;
  name: string;
  description: string;
  createdAt: string;
  updatedAt: string;
  nodes: FlowgramWorkflowJSON['nodes'];
  edges: FlowgramWorkflowJSON['edges'];
  payloadText: string;
  activeEnvironmentId: string;
  environments: ProjectEnvironment[];
  snapshots: ProjectBoardSnapshotFile[];
  migrationNotes: string[];
}

export interface ImportProjectsResult {
  importedProjects: ProjectRecord[];
  migrationNotes: string[];
}

// ---- 内部工具函数 ----

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export function cloneJson<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

export function nowIso(): string {
  return new Date().toISOString();
}

export function slugify(value: string): string {
  const normalized = value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9一-龥]+/g, '-')
    .replace(/^-+|-+$/g, '');

  return normalized || 'project';
}

export function createId(prefix: string): string {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) {
    return `${prefix}-${crypto.randomUUID().slice(0, 8)}`;
  }

  return `${prefix}-${Math.random().toString(36).slice(2, 10)}`;
}

export function normalizeString(value: unknown, fallback: string): string {
  return typeof value === 'string' && value.trim() ? value.trim() : fallback;
}

export function normalizeJsonValue(value: unknown, fallback: JsonValue = {}): JsonValue {
  if (
    value === null ||
    typeof value === 'string' ||
    typeof value === 'number' ||
    typeof value === 'boolean'
  ) {
    return value;
  }

  if (Array.isArray(value)) {
    return value.map((item) => normalizeJsonValue(item, null));
  }

  if (isRecord(value)) {
    return Object.entries(value).reduce<Record<string, JsonValue>>((acc, [key, nextValue]) => {
      acc[key] = normalizeJsonValue(nextValue, null);
      return acc;
    }, {});
  }

  return fallback;
}

export function asJsonObject(value: unknown): Record<string, JsonValue> {
  const normalized = normalizeJsonValue(value, {});
  return isRecord(normalized) ? (normalized as Record<string, JsonValue>) : {};
}

export function deepMergeJson(baseValue: JsonValue, overrideValue: JsonValue): JsonValue {
  if (Array.isArray(overrideValue)) {
    return cloneJson(overrideValue);
  }

  if (isRecord(baseValue) && isRecord(overrideValue)) {
    const result: Record<string, JsonValue> = { ...baseValue } as Record<string, JsonValue>;

    Object.entries(overrideValue).forEach(([key, nextValue]) => {
      const currentValue = result[key] ?? null;
      const normalizedNextValue = normalizeJsonValue(nextValue, null);
      result[key] =
        isRecord(currentValue) && isRecord(normalizedNextValue)
          ? deepMergeJson(currentValue, normalizedNextValue)
          : cloneJson(normalizedNextValue);
    });

    return result;
  }

  return cloneJson(overrideValue);
}

// 仅供内部使用——不导出给外部
export type { ConnectionDefinition, JsonValue, WorkflowGraph, WorkflowNodeDefinition };
