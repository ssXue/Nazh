import type { ThemeMode } from '../components/app/types';

export type AccentPreset = 'ocean' | 'teal' | 'pine' | 'amber' | 'rose' | 'custom';

export interface AccentPresetOption {
  key: Exclude<AccentPreset, 'custom'>;
  label: string;
  hex: string;
}

interface RgbColor {
  r: number;
  g: number;
  b: number;
}

export const ACCENT_PRESET_OPTIONS: AccentPresetOption[] = [
  { key: 'ocean', label: '海蓝', hex: '#4a72c9' },
  { key: 'teal', label: '青绿', hex: '#3d8a86' },
  { key: 'pine', label: '松绿', hex: '#5d8d67' },
  { key: 'amber', label: '琥珀', hex: '#b3834e' },
  { key: 'rose', label: '玫瑰', hex: '#ad6b7b' },
];

const DEFAULT_CUSTOM_ACCENT = '#6f83d6';

function clampChannel(value: number): number {
  return Math.min(255, Math.max(0, Math.round(value)));
}

function normalizeHex(input: string): string {
  const trimmed = input.trim();
  const normalized = trimmed.startsWith('#') ? trimmed : `#${trimmed}`;
  const validHex = /^#([0-9a-fA-F]{6})$/.test(normalized);
  return validHex ? normalized.toLowerCase() : DEFAULT_CUSTOM_ACCENT;
}

function hexToRgb(hex: string): RgbColor {
  const normalized = normalizeHex(hex).slice(1);
  return {
    r: Number.parseInt(normalized.slice(0, 2), 16),
    g: Number.parseInt(normalized.slice(2, 4), 16),
    b: Number.parseInt(normalized.slice(4, 6), 16),
  };
}

function rgbToHex(color: RgbColor): string {
  return `#${[color.r, color.g, color.b]
    .map((channel) => clampChannel(channel).toString(16).padStart(2, '0'))
    .join('')}`;
}

function mixColors(color: RgbColor, target: RgbColor, ratio: number): RgbColor {
  return {
    r: color.r + (target.r - color.r) * ratio,
    g: color.g + (target.g - color.g) * ratio,
    b: color.b + (target.b - color.b) * ratio,
  };
}

function toRgba(color: RgbColor, alpha: number): string {
  return `rgba(${clampChannel(color.r)}, ${clampChannel(color.g)}, ${clampChannel(color.b)}, ${alpha})`;
}

export function getAccentHex(preset: AccentPreset, customHex: string): string {
  if (preset === 'custom') {
    return normalizeHex(customHex);
  }

  return ACCENT_PRESET_OPTIONS.find((option) => option.key === preset)?.hex ?? ACCENT_PRESET_OPTIONS[0].hex;
}

export function normalizeCustomAccentHex(input: string): string {
  return normalizeHex(input);
}

export function buildAccentThemeVariables(
  accentHex: string,
  themeMode: ThemeMode,
): Record<string, string> {
  const base = hexToRgb(accentHex);
  const darkBase = { r: 22, g: 24, b: 29 };
  const lightBase = { r: 248, g: 249, b: 251 };
  const inkTarget = themeMode === 'dark' ? lightBase : darkBase;
  const hoverTarget = themeMode === 'dark' ? lightBase : darkBase;
  const nodeRhai =
    themeMode === 'dark'
      ? { r: 151, g: 167, b: 188 }
      : { r: 122, g: 137, b: 159 };

  const accentInk = rgbToHex(mixColors(base, inkTarget, themeMode === 'dark' ? 0.3 : 0.18));
  const accentHover = rgbToHex(mixColors(base, hoverTarget, themeMode === 'dark' ? 0.08 : 0.1));
  const accentGradientEnd = toRgba(
    mixColors(base, themeMode === 'dark' ? lightBase : { r: 255, g: 255, b: 255 }, themeMode === 'dark' ? 0.08 : 0.18),
    themeMode === 'dark' ? 0.8 : 0.94,
  );

  return {
    '--accent': normalizeHex(accentHex),
    '--accent-rgb': `${base.r} ${base.g} ${base.b}`,
    '--accent-ink': accentInk,
    '--accent-hover': accentHover,
    '--accent-soft': toRgba(base, themeMode === 'dark' ? 0.18 : 0.12),
    '--accent-soft-strong': toRgba(base, themeMode === 'dark' ? 0.26 : 0.18),
    '--accent-border': toRgba(base, themeMode === 'dark' ? 0.42 : 0.24),
    '--accent-shadow': toRgba(base, themeMode === 'dark' ? 0.24 : 0.18),
    '--accent-gradient-start': toRgba(base, themeMode === 'dark' ? 0.88 : 0.96),
    '--accent-gradient-end': accentGradientEnd,
    '--node-native': normalizeHex(accentHex),
    '--node-code': rgbToHex(nodeRhai),
  };
}
