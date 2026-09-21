/**
 * Diagram helpers both apps use: SVG path strings from point lists, and the
 * column "flow" diagram (inputs → screens → outputs, sockets → channels →
 * buses → outputs) that draws each column as a stack of boxes joined by
 * curves.
 */
import { approxWidth, fitText, type ColorRef, type Diagram, type DiagramItem } from './ir';

const f = (n: number) => (Math.round(n * 100) / 100).toString();

/** `M x y L …` or, with `curve`, cubic segments with horizontal handles. */
export function pathData(points: [number, number][], curve = false, close = false): string {
  if (!points.length) return '';
  let d = `M ${f(points[0][0])} ${f(points[0][1])}`;
  for (let i = 1; i < points.length; i++) {
    const [x0, y0] = points[i - 1];
    const [x1, y1] = points[i];
    if (curve) {
      const dx = (x1 - x0) / 2;
      d += ` C ${f(x0 + dx)} ${f(y0)} ${f(x1 - dx)} ${f(y1)} ${f(x1)} ${f(y1)}`;
    } else d += ` L ${f(x1)} ${f(y1)}`;
  }
  return close ? `${d} Z` : d;
}

export interface FlowNode {
  id: string;
  label: string;
  sub?: string;
  color?: ColorRef;
}

export interface FlowColumn {
  title: string;
  nodes: FlowNode[];
}

export interface FlowLink {
  from: string;
  to: string;
  color?: ColorRef;
  /** Stroke width multiplier. */
  weight?: number;
  dash?: number[];
}

/**
 * Columns of boxes joined by curves. Boxes are laid out top to bottom in
 * each column; a link between two nodes in any columns is drawn from the
 * right edge of the earlier column's box to the left edge of the later one.
 */
export function flowDiagram(columns: FlowColumn[], links: FlowLink[], cw: number, opts: { dense?: boolean; title?: boolean } = {}): Diagram {
  const n = Math.max(1, columns.length);
  const maxRows = Math.max(1, ...columns.map((c) => c.nodes.length));
  const dense = opts.dense ?? maxRows > 22;
  const hasSub = !dense && columns.some((c) => c.nodes.some((x) => x.sub));
  const boxH = dense ? 10 : hasSub ? 20 : 13;
  const rowH = boxH + (dense ? 2 : 4);
  const gap = 34;
  const colW = (cw - gap * (n - 1)) / n;
  const top = 16;
  const items: DiagramItem[] = [];
  const pos = new Map<string, { x: number; y: number; col: number }>();
  const size = dense ? 5.5 : 7;

  columns.forEach((c, ci) => {
    const x = ci * (colW + gap);
    items.push({ t: 'text', x, y: 9, text: c.title.toUpperCase(), size: 6.5, bold: true, color: 'muted' });
    if (!c.nodes.length) items.push({ t: 'text', x, y: top + 9, text: '(none)', size: 6.5, color: 'muted' });
    c.nodes.forEach((nd, ri) => {
      const y = top + ri * rowH;
      pos.set(nd.id, { x, y: y + boxH / 2, col: ci });
      items.push({ t: 'rect', x, y, w: colW, h: boxH, fill: nd.color ?? 'accent', opacity: 0.14, stroke: 'rule', sw: 0.5, r: 1.5 });
      items.push({ t: 'rect', x, y, w: 2.5, h: boxH, fill: nd.color ?? 'accent' });
      const labelW = hasSub ? colW - 8 : colW - 8;
      items.push({ t: 'text', x: x + 5, y: y + (hasSub ? 8.5 : boxH / 2 + size * 0.36), text: fitText(nd.label, size, labelW, true), size, bold: true, color: 'text' });
      if (hasSub && nd.sub) items.push({ t: 'text', x: x + 5, y: y + 16.5, text: fitText(nd.sub, 5.5, colW - 8), size: 5.5, color: 'muted' });
    });
  });

  for (const l of links) {
    const a = pos.get(l.from);
    const b = pos.get(l.to);
    if (!a || !b) continue;
    const [from, to] = a.col <= b.col ? [a, b] : [b, a];
    if (from.col === to.col) continue;
    items.push({ t: 'path', points: [[from.x + colW, from.y], [to.x, to.y]], curve: true, stroke: l.color ?? 'accent', sw: 0.7 * (l.weight ?? 1), opacity: 0.55 });
  }

  return { w: cw, h: top + maxRows * rowH + 2, items };
}

/** A legend row of coloured squares with labels. */
export function legend(entries: { label: string; color: ColorRef }[], cw: number, y = 0): { items: DiagramItem[]; h: number } {
  const items: DiagramItem[] = [];
  let x = 0;
  let row = 0;
  const size = 6.5;
  for (const e of entries) {
    const w = 10 + approxWidth(e.label, size) + 12;
    if (x + w > cw && x > 0) {
      x = 0;
      row += 1;
    }
    const yy = y + row * 11;
    items.push({ t: 'rect', x, y: yy, w: 7, h: 7, fill: e.color });
    items.push({ t: 'text', x: x + 10, y: yy + 6, text: e.label, size, color: 'text' });
    x += w;
  }
  return { items, h: (row + 1) * 11 };
}
