import type { AiProviderView, AiSecretInput } from '../types';

export interface ProviderFormState {
  name: string;
  baseUrl: string;
  apiKey: string;
  defaultModel: string;
}

export const EMPTY_PROVIDER_FORM: ProviderFormState = {
  name: '',
  baseUrl: '',
  apiKey: '',
  defaultModel: '',
};

export type ProviderApiKeyMode = 'keep' | 'set' | 'clear';

function trimProviderForm(form: ProviderFormState): ProviderFormState {
  return {
    name: form.name.trim(),
    baseUrl: form.baseUrl.trim(),
    apiKey: form.apiKey.trim(),
    defaultModel: form.defaultModel.trim(),
  };
}

export function toProviderFormState(
  provider: Pick<AiProviderView, 'name' | 'baseUrl' | 'defaultModel'> | null | undefined,
): ProviderFormState {
  if (!provider) {
    return EMPTY_PROVIDER_FORM;
  }

  return {
    name: provider.name,
    baseUrl: provider.baseUrl,
    apiKey: '',
    defaultModel: provider.defaultModel,
  };
}

export function resolveProviderApiKeyMode(
  formApiKey: string,
  editingProviderId: string | null,
  clearSavedApiKey: boolean,
): ProviderApiKeyMode {
  if (editingProviderId === null) {
    return 'set';
  }

  if (formApiKey.trim()) {
    return 'set';
  }

  return clearSavedApiKey ? 'clear' : 'keep';
}

export function resolveProviderApiKeyInput(
  formApiKey: string,
  editingProviderId: string | null,
  clearSavedApiKey: boolean,
): AiSecretInput {
  const mode = resolveProviderApiKeyMode(formApiKey, editingProviderId, clearSavedApiKey);
  if (mode === 'clear') {
    return { kind: 'clear' } as AiSecretInput;
  }
  if (mode === 'keep') {
    return { kind: 'keep' } as AiSecretInput;
  }
  return { kind: 'set', value: formApiKey.trim() } as AiSecretInput;
}

export function hasPendingProviderChanges(
  provider: Pick<AiProviderView, 'name' | 'baseUrl' | 'defaultModel'> | null | undefined,
  form: ProviderFormState,
  editingProviderId: string | null,
  clearSavedApiKey: boolean,
): boolean {
  if (!provider || editingProviderId === null) {
    return false;
  }

  const nextForm = trimProviderForm(form);
  if (provider.name !== nextForm.name) {
    return true;
  }
  if (provider.baseUrl !== nextForm.baseUrl) {
    return true;
  }
  if (provider.defaultModel !== nextForm.defaultModel) {
    return true;
  }

  return resolveProviderApiKeyMode(nextForm.apiKey, editingProviderId, clearSavedApiKey) !== 'keep';
}
