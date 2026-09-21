/**
 * The documentation as data. A document is a list of blocks — cover,
 * headings, paragraphs, key/value pairs, stat tiles, tables and diagrams —
 * that the PDF renderer (pdf-lib) and the HTML renderer both draw, so the
 * two exports say the same thing and a new section is written once.
 *
 * Diagrams are built in page points at the content width the paper gives
 * (`DocBuilder.cw`), y down like SVG, with text sizes and stroke widths in
 * points; a renderer scales the geometry at most a little (Letter is 3%
 * wider than A4, an over-tall diagram shrinks to its page).
 *
 * Colours are theme tokens (`text`, `accent`, `canvas`, `palette:2`, …) or
 * literal `#rrggbb`, resolved by the renderer against the chosen theme.
 */

export type ColorRef = string;

export interface Cell {
  text: string;
  /** A small colour square before the text (a desk colour, a layer colour). */
  swatch?: ColorRef;
  /** Background fill for the cell. */
  fill?: ColorRef;
  /** 0..1 — the cell is tinted with the accent at that strength (a heatmap). */
  heat?: number;
  /** A filled dot instead of / before the text; a colour ref or `true` for the accent. */
  dot?: boolean | ColorRef;
  bold?: boolean;
  muted?: boolean;
  color?: ColorRef;
  align?: 'l' | 'c' | 'r';
}
export type CellLike = string | Cell;

export interface TableOptions {
  /** Relative column widths (any unit; scaled to the content width). */
  widths?: number[];
  size?: 'tiny' | 'small' | 'normal';
  align?: ('l' | 'c' | 'r')[];
  /** A colour square in the header of that column (a bus colour above a send column). */
  headerSwatches?: (ColorRef | undefined)[];
  /** Rotate nothing, but keep the header short: renderers may wrap header cells. */
  stripes?: boolean;
}

export type DiagramItem =
  | { t: 'rect'; x: number; y: number; w: number; h: number; fill?: ColorRef; stroke?: ColorRef; sw?: number; dash?: number[]; opacity?: number; strokeOpacity?: number; r?: number }
  | { t: 'text'; x: number; y: number; text: string; size: number; color?: ColorRef; bold?: boolean; anchor?: 'start' | 'middle' | 'end'; opacity?: number; mono?: boolean }
  | { t: 'line'; x1: number; y1: number; x2: number; y2: number; stroke?: ColorRef; sw?: number; dash?: number[]; opacity?: number }
  | { t: 'path'; points: [number, number][]; stroke?: ColorRef; sw?: number; fill?: ColorRef; opacity?: number; curve?: boolean; close?: boolean }
  | { t: 'circle'; cx: number; cy: number; r: number; fill?: ColorRef; stroke?: ColorRef; sw?: number; opacity?: number };

export interface Diagram {
  w: number;
  h: number;
  items: DiagramItem[];
  /** Draw the theme's canvas colour behind everything (a screen, a rack). */
  canvas?: boolean;
}

export interface Tile {
  label: string;
  value: string;
  hint?: string;
}

export interface GlossaryItem {
  term: string;
  text: string;
}

export type Block =
  | { kind: 'cover'; title: string; subtitle: string; production: [string, string][]; tiles: Tile[]; lines: string[]; notes?: string }
  | { kind: 'h1'; text: string; id: string }
  | { kind: 'h2'; text: string }
  | { kind: 'h3'; text: string }
  | { kind: 'p'; text: string; muted?: boolean }
  | { kind: 'kv'; rows: [string, string][] }
  | { kind: 'tiles'; items: Tile[] }
  | ({ kind: 'table'; headers: string[]; rows: CellLike[][] } & TableOptions)
  | { kind: 'diagram'; d: Diagram; caption?: string }
  | { kind: 'glossary'; entries: GlossaryItem[] }
  | { kind: 'gap'; h: number }
  | { kind: 'pagebreak' };

export interface Document {
  title: string;
  subtitle: string;
  producer: string;
  blocks: Block[];
}

const slug = (s: string) => s.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'section';

export class DocBuilder {
  blocks: Block[] = [];
  private ids = new Map<string, number>();

  /** Content width in points: diagrams are built to fit it. */
  readonly cw: number;

  constructor(cw: number) {
    this.cw = cw;
  }

  cover(b: Omit<Extract<Block, { kind: 'cover' }>, 'kind'>) {
    this.blocks.push({ kind: 'cover', ...b });
  }
  h1(text: string) {
    const base = slug(text);
    const n = (this.ids.get(base) ?? 0) + 1;
    this.ids.set(base, n);
    this.blocks.push({ kind: 'h1', text, id: n === 1 ? base : `${base}-${n}` });
  }
  h2(text: string) {
    this.blocks.push({ kind: 'h2', text });
  }
  h3(text: string) {
    this.blocks.push({ kind: 'h3', text });
  }
  p(text: string, muted = false) {
    this.blocks.push({ kind: 'p', text, muted });
  }
  kv(rows: [string, string][]) {
    this.blocks.push({ kind: 'kv', rows });
  }
  tiles(items: Tile[]) {
    this.blocks.push({ kind: 'tiles', items });
  }
  table(headers: string[], rows: CellLike[][], opts: TableOptions = {}) {
    this.blocks.push({ kind: 'table', headers, rows, ...opts });
  }
  diagram(d: Diagram, caption?: string) {
    this.blocks.push({ kind: 'diagram', d, caption });
  }
  glossary(entries: GlossaryItem[]) {
    this.blocks.push({ kind: 'glossary', entries });
  }
  gap(h = 8) {
    this.blocks.push({ kind: 'gap', h });
  }
  pagebreak() {
    this.blocks.push({ kind: 'pagebreak' });
  }
}

export const cellText = (c: CellLike): string => (typeof c === 'string' ? c : c.text);
export const cell = (c: CellLike): Cell => (typeof c === 'string' ? { text: c } : c);

/** Roughly how wide a Helvetica string is, for laying out diagram labels. */
export function approxWidth(text: string, size: number, bold = false): number {
  let w = 0;
  for (const ch of text) {
    if (ch === ' ') w += 0.28;
    else if ('iljtfI.,:;\'"!|'.includes(ch)) w += 0.3;
    else if ('mwMW'.includes(ch)) w += 0.85;
    else if (ch >= 'A' && ch <= 'Z') w += 0.68;
    else if (ch >= '0' && ch <= '9') w += 0.56;
    else w += 0.54;
  }
  return w * size * (bold ? 1.06 : 1);
}

/** Cut a label so it fits `width` at `size`, with an ellipsis. */
export function fitText(text: string, size: number, width: number, bold = false): string {
  if (approxWidth(text, size, bold) <= width) return text;
  let s = text;
  while (s.length > 1 && approxWidth(`${s}…`, size, bold) > width) s = s.slice(0, -1);
  return `${s.trimEnd()}…`;
}
