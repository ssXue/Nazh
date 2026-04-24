import { describe, expect, it } from 'vitest';

import {
  EMPTY_PROVIDER_FORM,
  hasPendingProviderChanges,
  resolveProviderApiKeyInput,
  resolveProviderApiKeyMode,
  toProviderFormState,
} from '../ai-config';

describe('ai-config helpers', () => {
  it('将 provider 视图转成可编辑表单且不回填 api key', () => {
    expect(
      toProviderFormState({
        name: 'DeepSeek',
        baseUrl: 'https://api.deepseek.com',
        defaultModel: 'deepseek-v4-flash',
      }),
    ).toEqual({
      name: 'DeepSeek',
      baseUrl: 'https://api.deepseek.com',
      apiKey: '',
      defaultModel: 'deepseek-v4-flash',
    });
  });

  it('新增 provider 时总是要求 set api key', () => {
    expect(resolveProviderApiKeyMode('sk-123', null, false)).toBe('set');
    expect(resolveProviderApiKeyInput('sk-123', null, false)).toEqual({
      kind: 'set',
      value: 'sk-123',
    });
  });

  it('编辑 provider 时支持 keep clear set 三种 api key 策略', () => {
    expect(resolveProviderApiKeyMode('', 'provider-1', false)).toBe('keep');
    expect(resolveProviderApiKeyInput('', 'provider-1', false)).toEqual({ kind: 'keep' });

    expect(resolveProviderApiKeyMode('', 'provider-1', true)).toBe('clear');
    expect(resolveProviderApiKeyInput('', 'provider-1', true)).toEqual({ kind: 'clear' });

    expect(resolveProviderApiKeyMode('sk-new', 'provider-1', false)).toBe('set');
    expect(resolveProviderApiKeyInput('sk-new', 'provider-1', false)).toEqual({
      kind: 'set',
      value: 'sk-new',
    });
  });

  it('仅在编辑时根据表单和 api key 策略识别未保存变更', () => {
    const provider = {
      name: 'DeepSeek',
      baseUrl: 'https://api.deepseek.com',
      defaultModel: 'deepseek-v4-flash',
    };

    expect(hasPendingProviderChanges(provider, EMPTY_PROVIDER_FORM, null, false)).toBe(false);
    expect(
      hasPendingProviderChanges(provider, toProviderFormState(provider), 'provider-1', false),
    ).toBe(false);
    expect(
      hasPendingProviderChanges(
        provider,
        {
          ...toProviderFormState(provider),
          defaultModel: 'deepseek-v4-pro',
        },
        'provider-1',
        false,
      ),
    ).toBe(true);
    expect(
      hasPendingProviderChanges(provider, toProviderFormState(provider), 'provider-1', true),
    ).toBe(true);
  });
});
