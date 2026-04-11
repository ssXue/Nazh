//! 桌面偏好设置的 localStorage 持久化读取。

import type { MotionMode, StartupPage, ThemeMode } from '../components/app/types';
import { ACCENT_PRESET_OPTIONS, normalizeCustomAccentHex, type AccentPreset } from './theme';

/** localStorage 键名：主题模式。 */
export const THEME_STORAGE_KEY = 'nazh.theme';
/** localStorage 键名：强调色预设。 */
export const ACCENT_PRESET_STORAGE_KEY = 'nazh.accent-preset';
/** localStorage 键名：自定义强调色十六进制值。 */
export const CUSTOM_ACCENT_STORAGE_KEY = 'nazh.custom-accent';
/** localStorage 键名：动效模式。 */
export const MOTION_MODE_STORAGE_KEY = 'nazh.motion-mode';
/** localStorage 键名：启动页面。 */
export const STARTUP_PAGE_STORAGE_KEY = 'nazh.startup-page';
/** localStorage 键名：工程工作路径。 */
export const PROJECT_WORKSPACE_PATH_STORAGE_KEY = 'nazh.project-workspace-path';

/** 从 localStorage 读取主题模式，缺省时跟随系统偏好。 */
export function getInitialThemeMode(): ThemeMode {
  if (typeof window === 'undefined') {
    return 'light';
  }

  try {
    const storedTheme = window.localStorage.getItem(THEME_STORAGE_KEY);
    if (storedTheme === 'light' || storedTheme === 'dark') {
      return storedTheme;
    }
  } catch {
    // Ignore storage access failures and fall back to system preference.
  }

  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

/** 从 localStorage 读取强调色预设，缺省时使用第一个预设。 */
export function getInitialAccentPreset(): AccentPreset {
  if (typeof window === 'undefined') {
    return ACCENT_PRESET_OPTIONS[0].key;
  }

  try {
    const storedPreset = window.localStorage.getItem(ACCENT_PRESET_STORAGE_KEY);
    if (
      storedPreset === 'custom' ||
      ACCENT_PRESET_OPTIONS.some((option) => option.key === storedPreset)
    ) {
      return storedPreset as AccentPreset;
    }
  } catch {
    // Ignore storage access failures and fall back to defaults.
  }

  return ACCENT_PRESET_OPTIONS[0].key;
}

/** 从 localStorage 读取自定义强调色，缺省时使用第一个预设的颜色。 */
export function getInitialCustomAccentHex(): string {
  if (typeof window === 'undefined') {
    return normalizeCustomAccentHex(ACCENT_PRESET_OPTIONS[0].hex);
  }

  try {
    const storedHex = window.localStorage.getItem(CUSTOM_ACCENT_STORAGE_KEY);
    if (storedHex) {
      return normalizeCustomAccentHex(storedHex);
    }
  } catch {
    // Ignore storage access failures and fall back to defaults.
  }

  return normalizeCustomAccentHex(ACCENT_PRESET_OPTIONS[0].hex);
}

/** 从 localStorage 读取动效模式，缺省时跟随系统 prefers-reduced-motion。 */
export function getInitialMotionMode(): MotionMode {
  if (typeof window === 'undefined') {
    return 'full';
  }

  try {
    const storedMotionMode = window.localStorage.getItem(MOTION_MODE_STORAGE_KEY);
    if (storedMotionMode === 'full' || storedMotionMode === 'reduced') {
      return storedMotionMode;
    }
  } catch {
    // Ignore storage access failures and fall back to system preference.
  }

  return window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 'reduced' : 'full';
}

/** 从 localStorage 读取启动页面，缺省为 dashboard。 */
export function getInitialStartupPage(): StartupPage {
  if (typeof window === 'undefined') {
    return 'dashboard';
  }

  try {
    const storedPage = window.localStorage.getItem(STARTUP_PAGE_STORAGE_KEY);
    if (storedPage === 'dashboard' || storedPage === 'boards') {
      return storedPage;
    }
  } catch {
    // Ignore storage access failures and fall back to defaults.
  }

  return 'dashboard';
}

/** 从 localStorage 读取工程工作路径，缺省时返回空字符串（使用默认目录）。 */
export function getInitialProjectWorkspacePath(): string {
  if (typeof window === 'undefined') {
    return '';
  }

  try {
    return window.localStorage.getItem(PROJECT_WORKSPACE_PATH_STORAGE_KEY)?.trim() ?? '';
  } catch {
    return '';
  }
}
