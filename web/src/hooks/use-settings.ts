//! 桌面偏好设置管理 hook。

import { useEffect, useMemo, useState } from 'react';

import type { MotionMode, StartupPage, ThemeMode, UiDensity } from '../components/app/types';
import {
  ACCENT_PRESET_STORAGE_KEY,
  CUSTOM_ACCENT_STORAGE_KEY,
  MOTION_MODE_STORAGE_KEY,
  PROJECT_WORKSPACE_PATH_STORAGE_KEY,
  STARTUP_PAGE_STORAGE_KEY,
  THEME_STORAGE_KEY,
  UI_DENSITY_STORAGE_KEY,
  getInitialAccentPreset,
  getInitialCustomAccentHex,
  getInitialProjectWorkspacePath,
  getInitialMotionMode,
  getInitialStartupPage,
  getInitialThemeMode,
  getInitialUiDensity,
} from '../lib/settings';
import {
  buildAccentThemeVariables,
  getAccentHex,
  normalizeCustomAccentHex,
  type AccentPreset,
} from '../lib/theme';

/** 所有偏好设置的当前状态快照。 */
export interface SettingsState {
  /** 当前主题模式（亮色 / 暗色）。 */
  themeMode: ThemeMode;
  /** 当前强调色预设键。 */
  accentPreset: AccentPreset;
  /** 自定义强调色十六进制值。 */
  customAccentHex: string;
  /** 经过计算后的最终强调色十六进制值。 */
  accentHex: string;
  /** 当前界面密度。 */
  densityMode: UiDensity;
  /** 当前动效模式。 */
  motionMode: MotionMode;
  /** 启动时默认显示的页面。 */
  startupPage: StartupPage;
  /** 工程工作路径；空字符串表示使用应用默认目录。 */
  projectWorkspacePath: string;
  /** 完整的强调色 CSS 变量映射（键为 CSS 变量名，值为颜色字符串）。 */
  accentThemeVariables: Record<string, string>;
}

/** 操作偏好设置的回调集合。 */
export interface SettingsActions {
  /** 切换主题模式。 */
  setThemeMode: (mode: ThemeMode) => void;
  /** 更改强调色预设。 */
  setAccentPreset: (preset: AccentPreset) => void;
  /** 更改自定义强调色（同时自动将预设切换为 custom）。 */
  setCustomAccentHex: (hex: string) => void;
  /** 更改界面密度。 */
  setDensityMode: (mode: UiDensity) => void;
  /** 更改动效模式。 */
  setMotionMode: (mode: MotionMode) => void;
  /** 更改启动页面。 */
  setStartupPage: (page: StartupPage) => void;
  /** 更改工程工作路径。 */
  setProjectWorkspacePath: (path: string) => void;
  /** 在亮色与暗色主题之间快速切换。 */
  toggleTheme: () => void;
}

/** 偏好设置 hook 的完整返回类型（状态 + 操作）。 */
export type UseSettingsResult = SettingsState & SettingsActions;

/**
 * 管理所有桌面偏好设置的状态，并将变更同步写入 localStorage
 * 以及 document 的 dataset / CSS 变量，使应用外观即时响应。
 */
export function useSettings(): UseSettingsResult {
  const [themeMode, setThemeMode] = useState<ThemeMode>(getInitialThemeMode);
  const [accentPreset, setAccentPreset] = useState<AccentPreset>(getInitialAccentPreset);
  const [customAccentHex, setCustomAccentHexRaw] = useState<string>(getInitialCustomAccentHex);
  const [densityMode, setDensityMode] = useState<UiDensity>(getInitialUiDensity);
  const [motionMode, setMotionMode] = useState<MotionMode>(getInitialMotionMode);
  const [startupPage, setStartupPage] = useState<StartupPage>(getInitialStartupPage);
  const [projectWorkspacePath, setProjectWorkspacePath] = useState<string>(
    getInitialProjectWorkspacePath,
  );

  // 计算最终强调色十六进制值。
  const accentHex = useMemo(
    () => getAccentHex(accentPreset, customAccentHex),
    [accentPreset, customAccentHex],
  );

  // 计算完整的强调色 CSS 变量映射。
  const accentThemeVariables = useMemo(
    () => buildAccentThemeVariables(accentHex, themeMode),
    [accentHex, themeMode],
  );

  // 主题模式变更时更新 document dataset 并持久化。
  useEffect(() => {
    document.documentElement.dataset.theme = themeMode;

    try {
      window.localStorage.setItem(THEME_STORAGE_KEY, themeMode);
    } catch {
      // 受限运行时下忽略存储失败。
    }
  }, [themeMode]);

  // 强调色变更时将 CSS 变量写入根元素并持久化。
  useEffect(() => {
    Object.entries(accentThemeVariables).forEach(([key, value]) => {
      document.documentElement.style.setProperty(key, value);
    });

    try {
      window.localStorage.setItem(ACCENT_PRESET_STORAGE_KEY, accentPreset);
      window.localStorage.setItem(CUSTOM_ACCENT_STORAGE_KEY, customAccentHex);
    } catch {
      // 受限运行时下忽略存储失败。
    }
  }, [accentPreset, accentThemeVariables, customAccentHex]);

  // 界面密度变更时更新 document dataset 并持久化。
  useEffect(() => {
    document.documentElement.dataset.uiDensity = densityMode;

    try {
      window.localStorage.setItem(UI_DENSITY_STORAGE_KEY, densityMode);
    } catch {
      // 受限运行时下忽略存储失败。
    }
  }, [densityMode]);

  // 动效模式变更时更新 document dataset 并持久化。
  useEffect(() => {
    document.documentElement.dataset.motionMode = motionMode;

    try {
      window.localStorage.setItem(MOTION_MODE_STORAGE_KEY, motionMode);
    } catch {
      // 受限运行时下忽略存储失败。
    }
  }, [motionMode]);

  // 启动页面变更时持久化。
  useEffect(() => {
    try {
      window.localStorage.setItem(STARTUP_PAGE_STORAGE_KEY, startupPage);
    } catch {
      // 受限运行时下忽略存储失败。
    }
  }, [startupPage]);

  // 工程工作路径变更时持久化。
  useEffect(() => {
    try {
      window.localStorage.setItem(
        PROJECT_WORKSPACE_PATH_STORAGE_KEY,
        projectWorkspacePath.trim(),
      );
    } catch {
      // 受限运行时下忽略存储失败。
    }
  }, [projectWorkspacePath]);

  /** 设置自定义强调色，并自动将预设切换为 custom。 */
  function setCustomAccentHex(hex: string) {
    setAccentPreset('custom');
    setCustomAccentHexRaw(normalizeCustomAccentHex(hex));
  }

  /** 在亮色与暗色主题之间快速切换。 */
  function toggleTheme() {
    setThemeMode((current) => (current === 'dark' ? 'light' : 'dark'));
  }

  return {
    themeMode,
    accentPreset,
    customAccentHex,
    accentHex,
    densityMode,
    motionMode,
    startupPage,
    projectWorkspacePath,
    accentThemeVariables,
    setThemeMode,
    setAccentPreset,
    setCustomAccentHex,
    setDensityMode,
    setMotionMode,
    setStartupPage,
    setProjectWorkspacePath,
    toggleTheme,
  };
}
