/**
 * Themes and paper for the documentation. A theme is a handful of colours,
 * a font family and a type scale; both renderers read the same one, so a
 * PDF and an HTML export in the same theme look alike.
 */

export type ThemeId = 'light' | 'dark' | 'ink' | 'contrast' | 'classic';

export interface Theme {
  id: ThemeId;
  label: string;
  description: string;
  page: string;
  text: string;
  muted: string;
  heading: string;
  accent: string;
  rule: string;
  tableHead: string;
  tableHeadText: string;
  stripe: string;
  tile: string;
  canvas: string;
  canvasLine: string;
  canvasText: string;
  good: string;
  warn: string;
  bad: string;
  /** Layer / series colours. */
  palette: string[];
  font: 'sans' | 'serif' | 'mono';
  /** Multiplies every type size. */
  scale: number;
  /** No area fills: outlined tiles, plain table headers, for photocopies. */
  ink: boolean;
}

const PALETTE = ['#2e9ee0', '#3aa675', '#f5a524', '#e05a9e', '#8c70e0', '#bf7a2e', '#3bc0d6', '#c9d13a'];

export const THEMES: Record<ThemeId, Theme> = {
  light: {
    id: 'light',
    label: 'Light',
    description: 'White paper, blue accent. The default.',
    page: '#ffffff',
    text: '#1a1d24',
    muted: '#6b7280',
    heading: '#0f1218',
    accent: '#2f7fc4',
    rule: '#c9ced8',
    tableHead: '#eef1f6',
    tableHeadText: '#1a1d24',
    stripe: '#f6f7fa',
    tile: '#f1f4f9',
    canvas: '#1b1f28',
    canvasLine: '#6b7280',
    canvasText: '#c3c9d4',
    good: '#2f8f5b',
    warn: '#c98a12',
    bad: '#d13a3a',
    palette: PALETTE,
    font: 'sans',
    scale: 1,
    ink: false,
  },
  dark: {
    id: 'dark',
    label: 'Dark',
    description: 'The app’s own colours, for reading on a screen at the desk.',
    page: '#0f1218',
    text: '#e6ebf2',
    muted: '#8e99ab',
    heading: '#ffffff',
    accent: '#2f9ee0',
    rule: '#2b3140',
    tableHead: '#1b202b',
    tableHeadText: '#e6ebf2',
    stripe: '#141821',
    tile: '#161b25',
    canvas: '#05070a',
    canvasLine: '#4a5361',
    canvasText: '#aab3c2',
    good: '#3aa675',
    warn: '#f5a524',
    bad: '#e5484d',
    palette: PALETTE,
    font: 'sans',
    scale: 1,
    ink: false,
  },
  ink: {
    id: 'ink',
    label: 'Ink saver',
    description: 'Black on white, outlines instead of fills, for photocopies and cheap printers.',
    page: '#ffffff',
    text: '#000000',
    muted: '#555555',
    heading: '#000000',
    accent: '#000000',
    rule: '#000000',
    tableHead: '#ffffff',
    tableHeadText: '#000000',
    stripe: '#ffffff',
    tile: '#ffffff',
    canvas: '#ffffff',
    canvasLine: '#000000',
    canvasText: '#000000',
    good: '#000000',
    warn: '#000000',
    bad: '#000000',
    palette: ['#000000', '#555555', '#888888', '#333333', '#777777', '#aaaaaa', '#444444', '#999999'],
    font: 'sans',
    scale: 1,
    ink: true,
  },
  contrast: {
    id: 'contrast',
    label: 'High contrast',
    description: 'Larger type, black on white, strong rules, for reading under a work light.',
    page: '#ffffff',
    text: '#000000',
    muted: '#333333',
    heading: '#000000',
    accent: '#0040a0',
    rule: '#000000',
    tableHead: '#000000',
    tableHeadText: '#ffffff',
    stripe: '#f0f0f0',
    tile: '#f0f0f0',
    canvas: '#000000',
    canvasLine: '#ffffff',
    canvasText: '#ffffff',
    good: '#006b2f',
    warn: '#a05a00',
    bad: '#b00020',
    palette: ['#0040a0', '#008040', '#c06000', '#a01070', '#5030b0', '#804000', '#007080', '#606000'],
    font: 'sans',
    scale: 1.15,
    ink: false,
  },
  classic: {
    id: 'classic',
    label: 'Classic',
    description: 'Serif type on warm paper, a brown accent, for a printed show book.',
    page: '#fbf8f1',
    text: '#2b2520',
    muted: '#7a6f64',
    heading: '#2b2520',
    accent: '#8a5a2b',
    rule: '#d6ccbd',
    tableHead: '#efe7d8',
    tableHeadText: '#2b2520',
    stripe: '#f5f0e6',
    tile: '#f3ede1',
    canvas: '#2b2520',
    canvasLine: '#8a7f72',
    canvasText: '#e5dccb',
    good: '#4f7a3a',
    warn: '#b3782a',
    bad: '#a83a2c',
    palette: ['#8a5a2b', '#4f7a3a', '#b3782a', '#a83a2c', '#5c5a8a', '#2f6f7a', '#8a2b6a', '#6b6b2b'],
    font: 'serif',
    scale: 1,
    ink: false,
  },
};

export const THEME_LIST: Theme[] = Object.values(THEMES);

export type PaperId = 'a4' | 'letter';

export interface Paper {
  id: PaperId;
  label: string;
  w: number;
  h: number;
}

export const PAPERS: Record<PaperId, Paper> = {
  a4: { id: 'a4', label: 'A4', w: 595.28, h: 841.89 },
  letter: { id: 'letter', label: 'US Letter', w: 612, h: 792 },
};

export const MARGIN = 42;

export const contentWidth = (paper: PaperId): number => PAPERS[paper].w - 2 * MARGIN;

/** A colour token or literal to `#rrggbb`. */
export function resolveColor(theme: Theme, ref: string | undefined, fallback = theme.text): string {
  if (!ref) return fallback;
  if (ref.startsWith('#')) return ref;
  if (ref.startsWith('palette:')) {
    const i = Number(ref.slice(8)) || 0;
    return theme.palette[i % theme.palette.length];
  }
  const v = (theme as unknown as Record<string, unknown>)[ref];
  return typeof v === 'string' && v.startsWith('#') ? v : fallback;
}

export function hexToRgb(hex: string): [number, number, number] {
  const h = hex.replace('#', '');
  const n = parseInt(h.length === 3 ? h.split('').map((c) => c + c).join('') : h, 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

export function rgbToHex([r, g, b]: [number, number, number]): string {
  return `#${[r, g, b].map((v) => Math.max(0, Math.min(255, Math.round(v))).toString(16).padStart(2, '0')).join('')}`;
}

/** `t` of the way from `a` to `b`. */
export function blend(a: string, b: string, t: number): string {
  const [r1, g1, b1] = hexToRgb(a);
  const [r2, g2, b2] = hexToRgb(b);
  const k = Math.max(0, Math.min(1, t));
  return rgbToHex([r1 + (r2 - r1) * k, g1 + (g2 - g1) * k, b1 + (b2 - b1) * k]);
}

/** Black or white, whichever reads on `bg`. */
export function onColor(bg: string): string {
  const [r, g, b] = hexToRgb(bg);
  return (r * 299 + g * 587 + b * 114) / 1000 > 140 ? '#000000' : '#ffffff';
}
