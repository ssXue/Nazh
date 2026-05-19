import type { AiGenerationParams, AiProviderView, ConnectionDefinition } from '../../types';

export interface FlowgramNodeSettingsPanelProps {
  nodeId: string;
  connections: ConnectionDefinition[];
  aiProviders: AiProviderView[];
  activeAiProviderId: string | null;
  copilotParams: AiGenerationParams;
}

export type NodeConfigMap = Record<string, unknown>;

export const FLOWGRAM_NODE_SETTINGS_PANEL_KEY = 'nazh-flowgram-node-settings';
