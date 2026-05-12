/// API Key 安全读取层。
///
/// 通过 IPC 按需从 Rust 加密存储读取 provider 的 API key。
/// 读取后立即用于构造 provider 实例，不缓存在全局变量或 store。

import { invoke } from '@tauri-apps/api/core';

const DEBUG_API_KEY = true;

function apiKeyLog(...args: unknown[]) {
  if (DEBUG_API_KEY) console.log('[ai/api-key]', ...args);
}

/// 读取指定 provider 的 API key。
///
/// 返回空字符串表示该 provider 未配置 API key。
/// 调用方应立即使用返回值构造 provider 实例，不应存储到长期变量。
export async function loadApiKey(providerId: string): Promise<string> {
  try {
    const apiKey = await invoke<string>('load_ai_api_key', { providerId });
    apiKeyLog('读取 API key 成功', { providerId, keyLen: apiKey.length });
    return apiKey;
  } catch (error) {
    apiKeyLog('读取 API key 失败', { providerId, error });
    return '';
  }
}
