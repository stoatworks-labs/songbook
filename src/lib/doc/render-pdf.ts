/**
 * Draws a `Document` with pdf-lib: standard fonts only (Helvetica, Times,
 * Courier), so text is mapped into WinAnsi first. Diagrams come in as SVG-
 * style geometry with y down and are drawn through pdf-lib's SVG path
 * support at whatever scale fits the page.
 */
import { PDFDocument, PDFFont, PDFPage, StandardFonts, rgb, type RGB } from 'pdf-lib';

import { cell, type Block, type Cell, type CellLike, type Diagram, type Document } from './ir';
import { MARGIN as M, PAPERS, blend, hexToRgb, onColor, resolveColor, type PaperId, type Theme } from './theme';
import { pathData } from './diagrams';

/** Into WinAnsi: the arrows and maths that the UI uses have plain spellings. */
export function clean(s: unknown): string {
  return String(s ?? '')
    .replace(/→/g, '->')
    .replace(/←/g, '<-')
    .replace(/×/g, 'x')
    .replace(/−/g, '-')
    .replace(/∞/g, 'inf')
    .replace(/·/g, '-')
    .replace(/≤/g, '<=')
    .replace(/≥/g, '>=')
    .replace(/[✓✔]/g, 'y')
    .replace(/[●]/g, '•')
    .replace(/[○◦]/g, 'o')
    .replace(/[✕✗]/g, 'x')
    .replace(/[^\x20-\x7e\xa0-\xff–—‘’“”•…]/g, '?');
}

const col = (hex: string): RGB => {
  const [r, g, b] = hexToRgb(hex);
  return rgb(r / 255, g / 255, b / 255);
};

const FONT_FILES: Record<Theme['font'], [StandardFonts, StandardFonts]> = {
  sans: [StandardFonts.Helvetica, StandardFonts.HelveticaBold],
  serif: [StandardFonts.TimesRoman, StandardFonts.TimesRomanBold],
  mono: [StandardFonts.Courier, StandardFonts.CourierBold],
};

class Pdf {
  doc!: PDFDocument;
  font!: PDFFont;
  bold!: PDFFont;
  mono!: PDFFont;
  page!: PDFPage;
  y = 0;
  pageNo = 0;
  readonly w: number;
  readonly h: number;
  readonly cw: number;
  readonly s: number;

  readonly theme: Theme;
  readonly title: string;

  constructor(theme: Theme, paper: PaperId, title: string) {
    this.theme = theme;
    this.title = title;
    this.w = PAPERS[paper].w;
    this.h = PAPERS[paper].h;
    this.cw = this.w - 2 * M;
    this.s = theme.scale;
  }

  static async create(theme: Theme, paper: PaperId, title: string, producer: string): Promise<Pdf> {
    const p = new Pdf(theme, paper, title);
    p.doc = await PDFDocument.create();
    const [regular, bold] = FONT_FILES[theme.font];
    p.font = await p.doc.embedFont(regular);
    p.bold = await p.doc.embedFont(bold);
    p.mono = await p.doc.embedFont(StandardFonts.Courier);
    p.doc.setTitle(title);
    p.doc.setProducer(producer);
    p.doc.setCreator(producer);
    p.newPage();
    return p;
  }

  c(ref: string | undefined, fallback = this.theme.text): RGB {
    return col(resolveColor(this.theme, ref, fallback));
  }

  newPage() {
    this.page = this.doc.addPage([this.w, this.h]);
    this.pageNo += 1;
    this.y = this.h - M;
    if (this.theme.page !== '#ffffff') this.page.drawRectangle({ x: 0, y: 0, width: this.w, height: this.h, color: col(this.theme.page) });
    const size = 8 * this.s;
    this.page.drawText(clean(this.title), { x: M, y: this.h - 24, size, font: this.font, color: col(this.theme.muted) });
    const pn = `${this.pageNo}`;
    this.page.drawText(pn, { x: this.w - M - this.font.widthOfTextAtSize(pn, size), y: this.h - 24, size, font: this.font, color: col(this.theme.muted) });
    this.page.drawLine({ start: { x: M, y: this.h - 30 }, end: { x: this.w - M, y: this.h - 30 }, thickness: this.theme.ink ? 0.8 : 0.5, color: col(this.theme.rule) });
  }

  /** Room left on this page above the bottom margin. */
  get avail(): number {
    return this.y - M;
  }

  need(h: number) {
    if (this.y - h < M) this.newPage();
  }

  wrap(text: string, size: number, font: PDFFont, width: number): string[] {
    const out: string[] = [];
    for (const para of clean(text).split('\n')) {
      let line = '';
      for (const word of para.split(' ')) {
        const cand = line ? `${line} ${word}` : word;
        if (font.widthOfTextAtSize(cand, size) > width && line) {
          out.push(line);
          line = word;
        } else line = cand;
      }
      out.push(line);
    }
    return out;
  }

  text(t: string, x: number, y: number, size: number, opts: { font?: PDFFont; color?: RGB; anchor?: 'start' | 'middle' | 'end'; opacity?: number } = {}) {
    const font = opts.font ?? this.font;
    const s = clean(t);
    let dx = 0;
    if (opts.anchor === 'middle') dx = -font.widthOfTextAtSize(s, size) / 2;
    else if (opts.anchor === 'end') dx = -font.widthOfTextAtSize(s, size);
    this.page.drawText(s, { x: x + dx, y, size, font, color: opts.color ?? col(this.theme.text), opacity: opts.opacity });
  }

  para(text: string, opts: { size?: number; color?: RGB; font?: PDFFont; indent?: number; width?: number } = {}) {
    const size = (opts.size ?? 9.5) * this.s;
    const font = opts.font ?? this.font;
    const indent = opts.indent ?? 0;
    for (const line of this.wrap(text, size, font, (opts.width ?? this.cw) - indent)) {
      this.need(size + 4);
      this.y -= size + 3;
      this.page.drawText(line, { x: M + indent, y: this.y, size, font, color: opts.color ?? col(this.theme.text) });
    }
  }

  h1(t: string) {
    const size = 18 * this.s;
    this.need(size + 30);
    this.y -= size + 8;
    this.page.drawRectangle({ x: M - 10, y: this.y - 3, width: 4, height: size + 4, color: col(this.theme.accent) });
    this.text(t, M, this.y, size, { font: this.bold, color: col(this.theme.heading) });
    this.y -= 12;
  }

  h2(t: string) {
    const size = 13 * this.s;
    this.need(size + 24);
    this.y -= size + 6;
    this.text(t, M, this.y, size, { font: this.bold, color: col(this.theme.heading) });
    this.y -= 6;
  }

  h3(t: string) {
    const size = 9.5 * this.s;
    this.need(size + 16);
    this.y -= size + 6;
    this.text(t.toUpperCase(), M, this.y, size, { font: this.bold, color: col(this.theme.muted) });
    this.y -= 4;
  }

  kv(rows: [string, string][], width = this.cw, x0 = M) {
    const size = 9.5 * this.s;
    const labelW = Math.min(120 * this.s, width * 0.35);
    for (const [k, v] of rows) {
      const lines = this.wrap(v, size, this.font, width - labelW - 6);
      const h = lines.length * (size + 3) + 2;
      this.need(h);
      this.text(k, x0, this.y - size, size * 0.95, { font: this.bold, color: col(this.theme.muted) });
      lines.forEach((l, i) => this.text(l, x0 + labelW, this.y - size - i * (size + 3), size));
      this.y -= h;
    }
  }

  tiles(items: { label: string; value: string; hint?: string }[]) {
    const per = Math.min(4, Math.max(1, items.length));
    const gap = 8;
    const tw = (this.cw - gap * (per - 1)) / per;
    const th = 46 * this.s;
    for (let i = 0; i < items.length; i += per) {
      const row = items.slice(i, i + per);
      this.need(th + gap);
      row.forEach((t, k) => {
        const x = M + k * (tw + gap);
        const y = this.y - th;
        if (this.theme.ink) this.page.drawRectangle({ x, y, width: tw, height: th, borderColor: col(this.theme.rule), borderWidth: 0.8 });
        else this.page.drawRectangle({ x, y, width: tw, height: th, color: col(this.theme.tile) });
        this.text(t.value, x + 8, y + th - 22 * this.s, 16 * this.s, { font: this.bold, color: col(this.theme.heading) });
        this.text(t.label, x + 8, y + 8, 7.5 * this.s, { color: col(this.theme.muted) });
        if (t.hint) this.text(t.hint, x + tw - 8, y + 8, 7 * this.s, { color: col(this.theme.muted), anchor: 'end' });
      });
      this.y -= th + gap;
    }
  }

  table(b: Extract<Block, { kind: 'table' }>) {
    const base = b.size === 'tiny' ? 7 : b.size === 'small' ? 7.5 : 8.5;
    const size = base * this.s;
    const ws = (() => {
      const w = b.widths ?? b.headers.map(() => 1);
      const sum = w.reduce((a, c) => a + c, 0);
      return w.map((x) => (x / sum) * this.cw);
    })();
    const lineH = size + 3;
    const pad = 3;
    const stripes = b.stripes !== false && !this.theme.ink;

    const drawRow = (cells: CellLike[], header: boolean, stripe: boolean) => {
      const cs = cells.map(cell);
      const font = header ? this.bold : this.font;
      const lines = cs.map((c, i) => {
        const extra = (c.swatch || (header && b.headerSwatches?.[i]) ? 10 : 0) + (c.dot && c.text ? 8 : 0);
        return this.wrap(c.text, size, c.bold ? this.bold : font, Math.max(8, ws[i] - 2 * pad - extra));
      });
      const h = Math.max(1, ...lines.map((l) => l.length)) * lineH + 5;
      const top = this.y;
      if (header) {
        if (!this.theme.ink) this.page.drawRectangle({ x: M, y: top - h, width: this.cw, height: h, color: col(this.theme.tableHead) });
      } else if (stripe) this.page.drawRectangle({ x: M, y: top - h, width: this.cw, height: h, color: col(this.theme.stripe) });
      let x = M;
      cs.forEach((c, i) => {
        const w = ws[i];
        let textColor = header ? col(this.theme.tableHeadText) : c.muted ? col(this.theme.muted) : this.c(c.color);
        if (!header && (c.fill || c.heat !== undefined) && !this.theme.ink) {
          const fill = c.fill ? resolveColor(this.theme, c.fill) : blend(this.theme.page, this.theme.accent, 0.1 + 0.75 * Math.max(0, Math.min(1, c.heat ?? 0)));
          this.page.drawRectangle({ x: x + 0.5, y: top - h + 0.5, width: w - 1, height: h - 1, color: col(fill) });
          if ((c.heat ?? 0) > 0.55 || c.fill) textColor = col(onColor(fill));
        } else if (!header && c.heat !== undefined && this.theme.ink && c.heat > 0) {
          // No fills in ink mode: a hairline under the cell says "set".
          this.page.drawLine({ start: { x: x + 2, y: top - h + 2 }, end: { x: x + w - 2, y: top - h + 2 }, thickness: 0.5 + 1.5 * c.heat, color: col(this.theme.text) });
        }
        let dx = pad;
        const sw = c.swatch ?? (header ? b.headerSwatches?.[i] : undefined);
        if (sw) {
          this.page.drawRectangle({ x: x + pad, y: top - lineH - 1, width: 7, height: size - 1, color: this.c(sw) });
          dx += 10;
        }
        if (c.dot) {
          const dc = c.dot === true ? col(this.theme.accent) : this.c(c.dot);
          if (c.text) {
            this.page.drawCircle({ x: x + dx + 3, y: top - lineH + size * 0.35, size: 2.6, color: dc });
            dx += 8;
          } else this.page.drawCircle({ x: x + w / 2, y: top - h / 2, size: Math.min(3.2, size * 0.36), color: dc });
        }
        const align = c.align ?? b.align?.[i] ?? 'l';
        const f = c.bold ? this.bold : font;
        lines[i].forEach((l, k) => {
          const ly = top - (k + 1) * lineH + 1;
          const tw = f.widthOfTextAtSize(l, size);
          const lx = align === 'r' ? x + w - pad - tw : align === 'c' ? x + (w - tw) / 2 : x + dx;
          this.page.drawText(l, { x: lx, y: ly, size, font: f, color: textColor });
        });
        x += w;
      });
      this.y -= h;
      this.page.drawLine({ start: { x: M, y: this.y }, end: { x: M + this.cw, y: this.y }, thickness: header ? 0.8 : 0.3, color: col(this.theme.rule) });
    };

    const rowHeight = (cells: CellLike[]) => {
      const cs = cells.map(cell);
      const n = Math.max(1, ...cs.map((c, i) => this.wrap(c.text, size, this.font, Math.max(8, ws[i] - 2 * pad - 10)).length));
      return n * lineH + 5;
    };

    this.need(rowHeight(b.headers) + (b.rows[0] ? rowHeight(b.rows[0]) : 0));
    drawRow(b.headers, true, false);
    b.rows.forEach((r, i) => {
      if (this.y - rowHeight(r) < M) {
        this.newPage();
        drawRow(b.headers, true, false);
      }
      drawRow(r, false, stripes && i % 2 === 1);
    });
    this.y -= 6;
  }

  diagram(d: Diagram, caption?: string) {
    let s = Math.min(this.cw / d.w, 1.06);
    const maxH = this.h - 2 * M - 40;
    if (d.h * s > maxH) s = maxH / d.h;
    const dh = d.h * s;
    this.need(dh + (caption ? 14 : 4));
    const x0 = M;
    const top = this.y;
    const X = (x: number) => x0 + x * s;
    const Y = (y: number) => top - y * s;
    if (d.canvas) this.page.drawRectangle({ x: x0, y: top - dh, width: d.w * s, height: dh, color: col(this.theme.canvas), borderColor: col(this.theme.canvasLine), borderWidth: 0.6 });
    for (const it of d.items) {
      switch (it.t) {
        case 'rect':
          this.page.drawRectangle({
            x: X(it.x),
            y: Y(it.y + it.h),
            width: it.w * s,
            height: it.h * s,
            color: it.fill ? this.c(it.fill) : undefined,
            opacity: it.opacity,
            borderColor: it.stroke ? this.c(it.stroke) : undefined,
            borderWidth: it.stroke ? (it.sw ?? 0.6) : 0,
            borderOpacity: it.strokeOpacity,
            borderDashArray: it.dash,
          });
          break;
        case 'text':
          this.text(it.text, X(it.x), Y(it.y), it.size * s, { font: it.mono ? this.mono : it.bold ? this.bold : this.font, color: this.c(it.color), anchor: it.anchor, opacity: it.opacity });
          break;
        case 'line':
          this.page.drawLine({ start: { x: X(it.x1), y: Y(it.y1) }, end: { x: X(it.x2), y: Y(it.y2) }, thickness: it.sw ?? 0.6, color: this.c(it.stroke, this.theme.rule), dashArray: it.dash, opacity: it.opacity });
          break;
        case 'path':
          this.page.drawSvgPath(pathData(it.points, it.curve, it.close), {
            x: x0,
            y: top,
            scale: s,
            borderColor: it.stroke ? this.c(it.stroke) : undefined,
            borderWidth: it.stroke ? (it.sw ?? 0.8) : 0,
            color: it.fill ? this.c(it.fill) : undefined,
            opacity: it.opacity,
            borderOpacity: it.opacity,
          });
          break;
        case 'circle':
          this.page.drawCircle({ x: X(it.cx), y: Y(it.cy), size: it.r * s, color: it.fill ? this.c(it.fill) : undefined, borderColor: it.stroke ? this.c(it.stroke) : undefined, borderWidth: it.stroke ? (it.sw ?? 0.6) : 0, opacity: it.opacity });
          break;
      }
    }
    this.y = top - dh - 4;
    if (caption) {
      this.y -= 8 * this.s;
      this.text(caption, M, this.y, 7.5 * this.s, { color: col(this.theme.muted) });
      this.y -= 4;
    }
    this.y -= 4;
  }

  cover(b: Extract<Block, { kind: 'cover' }>) {
    this.y -= 90;
    const titleSize = 28 * this.s;
    for (const line of this.wrap(b.title, titleSize, this.bold, this.cw)) {
      this.text(line, M, this.y, titleSize, { font: this.bold, color: col(this.theme.heading) });
      this.y -= titleSize + 4;
    }
    this.y -= 6;
    this.text(b.subtitle, M, this.y, 13 * this.s, { color: col(this.theme.muted) });
    this.y -= 28;
    if (b.production.length) {
      const size = 10 * this.s;
      const rowH = size + 7;
      const boxH = b.production.length * rowH + 14;
      const y0 = this.y - boxH;
      if (this.theme.ink) this.page.drawRectangle({ x: M, y: y0, width: this.cw, height: boxH, borderColor: col(this.theme.rule), borderWidth: 0.8 });
      else this.page.drawRectangle({ x: M, y: y0, width: this.cw, height: boxH, color: col(this.theme.tile) });
      this.page.drawRectangle({ x: M, y: y0, width: 4, height: boxH, color: col(this.theme.accent) });
      let y = this.y - 8 - size;
      for (const [k, v] of b.production) {
        this.text(k, M + 14, y, size * 0.9, { font: this.bold, color: col(this.theme.muted) });
        this.text(v, M + 14 + 130 * this.s, y, size, { color: col(this.theme.text) });
        y -= rowH;
      }
      this.y = y0 - 18;
    }
    this.tiles(b.tiles);
    this.y -= 10;
    for (const l of b.lines) this.para(l, { color: col(this.theme.muted) });
    if (b.notes) {
      this.y -= 10;
      this.h2('Show notes');
      this.para(b.notes);
    }
  }

  glossary(entries: { term: string; text: string }[]) {
    for (const g of entries) {
      this.need(40);
      this.y -= 6;
      this.para(g.term, { font: this.bold, size: 10 });
      this.para(g.text, { indent: 10 });
    }
  }

  render(doc: Document) {
    for (const b of doc.blocks) {
      switch (b.kind) {
        case 'cover':
          this.cover(b);
          break;
        case 'h1':
          this.h1(b.text);
          break;
        case 'h2':
          this.h2(b.text);
          break;
        case 'h3':
          this.h3(b.text);
          break;
        case 'p':
          this.para(b.text, { color: b.muted ? col(this.theme.muted) : undefined });
          break;
        case 'kv':
          this.kv(b.rows);
          break;
        case 'tiles':
          this.tiles(b.items);
          break;
        case 'table':
          this.table(b);
          break;
        case 'diagram':
          this.diagram(b.d, b.caption);
          break;
        case 'glossary':
          this.glossary(b.entries);
          break;
        case 'gap':
          this.y -= b.h;
          break;
        case 'pagebreak':
          this.newPage();
          break;
      }
    }
  }
}

export async function renderPdf(doc: Document, theme: Theme, paper: PaperId): Promise<Uint8Array> {
  const p = await Pdf.create(theme, paper, doc.title, doc.producer);
  p.render(doc);
  return p.doc.save();
}

export type { Cell };
