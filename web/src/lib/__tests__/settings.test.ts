// @vitest-environment jsdom

// settings 单元测试：验证 localStorage 持久化读取逻辑。
import { afterEach, beforeAll, describe, expect, it } from 'vitest';
import {
  getInitialStartupPage,
  getInitialThemeMode,
  getInitialUiDensity,
  STARTUP_PAGE_STORAGE_KEY,
  THEME_STORAGE_KEY,
  UI_DENSITY_STORAGE_KEY,
} from '../settings';

// jsdom 不实现 matchMedia；此存根让未命中 localStorage 时回退至 light。
beforeAll(() => {
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    value: (query: string): MediaQueryList =>
      ({
        matches: false,
        media: query,
        onchange: null,
        addListener: () => undefined,
        removeListener: () => undefined,
        addEventListener: () => undefined,
        removeEventListener: () => undefined,
        dispatchEvent: () => false,
      }) as MediaQueryList,
  });
});

afterEach(() => {
  localStorage.clear();
});

describe('getInitialThemeMode', () => {
  it('已存储 dark → 返回 dark', () => {
    localStorage.setItem(THEME_STORAGE_KEY, 'dark');
    expect(getInitialThemeMode()).toBe('dark');
  });

  it('无存储值 → 默认返回 light', () => {
    expect(getInitialThemeMode()).toBe('light');
  });

  it('无效存储值 → 默认返回 light', () => {
    localStorage.setItem(THEME_STORAGE_KEY, 'blue');
    expect(getInitialThemeMode()).toBe('light');
  });
});

describe('getInitialUiDensity', () => {
  it('已存储 compact → 返回 compact', () => {
    localStorage.setItem(UI_DENSITY_STORAGE_KEY, 'compact');
    expect(getInitialUiDensity()).toBe('compact');
  });

  it('无存储值 → 默认返回 comfortable', () => {
    expect(getInitialUiDensity()).toBe('comfortable');
  });
});

describe('getInitialStartupPage', () => {
  it('已存储 boards → 返回 boards', () => {
    localStorage.setItem(STARTUP_PAGE_STORAGE_KEY, 'boards');
    expect(getInitialStartupPage()).toBe('boards');
  });

  it('无存储值 → 默认返回 dashboard', () => {
    expect(getInitialStartupPage()).toBe('dashboard');
  });
});
