/**
 * Show documentation as a PDF, drawn with pdf-lib: cover, desk and I/O,
 * the input list (patch, preamp, name, colour, fader, assignments), the
 * send matrix, buses and outputs, DCAs and mute groups, scenes, cues, the
 * import/conversion notes, and the version history.
 */
import { PDFDocument, PDFFont, PDFPage, StandardFonts, rgb, type RGB } from 'pdf-lib';

import { BUS_KIND_LABEL, FADER_OFF, PLATFORM_LABEL, fmtDb, fmtPan, socketLabel, type Bus, type Commit, type Show } from '../types';

const A4 = { w: 595.28, h: 841.89 };
const M = 42;

const COLOR_RGB: Record<string, RGB> = {
  off: rgb(0.45, 0.47, 0.52),
  red: rgb(0.88, 0.27, 0.24),
  green: rgb(0.23, 0.7, 0.36),
  yellow: rgb(0.9, 0.75, 0.2),
  blue: rgb(0.23, 0.48, 0.88),
  purple: rgb(0.56, 0.36, 0.84),
  cyan: rgb(0.23, 0.75, 0.84),
  white: rgb(0.85, 0.85, 0.85),
  orange: rgb(0.9, 0.54, 0.18),
  pink: rgb(0.88, 0.35, 0.62),
};

function clean(s: unknown): string {
  return String(s ?? '')
    .replace(/[→]/g, '->')
    .replace(/[–—]/g, '-')
    .replace(/[×]/g, 'x')
    .replace(/[…]/g, '...')
    .replace(/[−]/g, '-')
    .replace(/[∞]/g, 'inf')
    .replace(/[·]/g, '-')
    .replace(/[^\x20-\x7e\xa0-\xff]/g, '?');
}

class Doc {
  doc!: PDFDocument;
  font!: PDFFont;
  bold!: PDFFont;
  mono!: PDFFont;
  page!: PDFPage;
  y = 0;
  pageNo = 0;
  title = '';

  static async create(title: string): Promise<Doc> {
    const d = new Doc();
    d.doc = await PDFDocument.create();
    d.font = await d.doc.embedFont(StandardFonts.Helvetica);
    d.bold = await d.doc.embedFont(StandardFonts.HelveticaBold);
    d.mono = await d.doc.embedFont(StandardFonts.Courier);
    d.title = title;
    d.doc.setTitle(title);
    d.doc.setProducer('Songbook');
    d.newPage();
    return d;
  }

  newPage() {
    this.page = this.doc.addPage([A4.w, A4.h]);
    this.pageNo += 1;
    this.y = A4.h - M;
    this.page.drawText(clean(this.title), { x: M, y: A4.h - 24, size: 8, font: this.font, color: rgb(0.5, 0.5, 0.5) });
    const pn = `${this.pageNo}`;
    this.page.drawText(pn, { x: A4.w - M - this.font.widthOfTextAtSize(pn, 8), y: A4.h - 24, size: 8, font: this.font, color: rgb(0.5, 0.5, 0.5) });
    this.page.drawLine({ start: { x: M, y: A4.h - 30 }, end: { x: A4.w - M, y: A4.h - 30 }, thickness: 0.5, color: rgb(0.8, 0.8, 0.8) });
  }

  need(h: number) {
    if (this.y - h < M) this.newPage();
  }

  h1(t: string) {
    this.need(40);
    this.y -= 22;
    this.page.drawText(clean(t), { x: M, y: this.y, size: 18, font: this.bold });
    this.y -= 10;
  }

  h2(t: string) {
    this.need(30);
    this.y -= 18;
    this.page.drawText(clean(t), { x: M, y: this.y, size: 13, font: this.bold });
    this.y -= 6;
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

  p(text: string, opts: { size?: number; color?: RGB; font?: PDFFont; indent?: number } = {}) {
    const size = opts.size ?? 9.5;
    const font = opts.font ?? this.font;
    const indent = opts.indent ?? 0;
    for (const line of this.wrap(text, size, font, A4.w - 2 * M - indent)) {
      this.need(size + 4);
      this.y -= size + 3;
      this.page.drawText(line, { x: M + indent, y: this.y, size, font, color: opts.color ?? rgb(0.1, 0.1, 0.1) });
    }
  }

  gap(h = 8) {
    this.y -= h;
  }

  /** Rows may carry a colour swatch as `{ swatch: 'red' }` in a cell. */
  table(headers: string[], rows: (string | { swatch?: string; text: string })[][], widths?: number[], size = 8.5) {
    const total = A4.w - 2 * M;
    const ws = widths ?? headers.map(() => total / headers.length);
    const cellText = (c: string | { swatch?: string; text: string }) => (typeof c === 'string' ? c : c.text);
    const drawRow = (cells: (string | { swatch?: string; text: string })[], font: PDFFont, bg?: RGB) => {
      const lines = cells.map((c, i) => this.wrap(cellText(c), size, font, ws[i] - 6 - (typeof c !== 'string' && c.swatch ? 10 : 0)));
      const h = Math.max(...lines.map((l) => l.length)) * (size + 3) + 5;
      this.need(h);
      if (bg) this.page.drawRectangle({ x: M, y: this.y - h, width: total, height: h, color: bg });
      let x = M;
      lines.forEach((ls, i) => {
        const cell = cells[i];
        let dx = 0;
        if (typeof cell !== 'string' && cell.swatch) {
          this.page.drawRectangle({ x: x + 3, y: this.y - size - 2, width: 7, height: size - 1, color: COLOR_RGB[cell.swatch] ?? COLOR_RGB.off });
          dx = 10;
        }
        ls.forEach((l, k) => this.page.drawText(l, { x: x + 3 + dx, y: this.y - (k + 1) * (size + 3) + 1, size, font }));
        x += ws[i];
      });
      this.y -= h;
      this.page.drawLine({ start: { x: M, y: this.y }, end: { x: A4.w - M, y: this.y }, thickness: 0.3, color: rgb(0.75, 0.75, 0.75) });
    };
    drawRow(headers, this.bold, rgb(0.92, 0.93, 0.95));
    for (const r of rows) drawRow(r, this.font);
    this.gap(6);
  }
}

export interface PdfOptions {
  includeSends: boolean;
  includeScenes: boolean;
  includeHistory: boolean;
  preparedBy: string;
  event: string;
}

const onOff = (v?: boolean) => (v === undefined ? '' : v ? 'on' : 'off');

export async function buildPdf(show: Show, commits: Commit[], opts: PdfOptions): Promise<Uint8Array> {
  const d = await Doc.create(`${show.meta.name} - ${PLATFORM_LABEL[show.platform]} ${show.system.model}`);
  const inputs = show.channels.filter((c) => c.kind === 'input');
  const others = show.channels.filter((c) => c.kind !== 'input');
  const bus = (id: string) => show.buses.find((b) => b.id === id);

  // ---- cover
  d.y -= 120;
  d.page.drawText(clean(show.meta.name), { x: M, y: d.y, size: 28, font: d.bold });
  d.y -= 30;
  d.page.drawText(clean(`${PLATFORM_LABEL[show.platform]} - ${show.system.model || 'model not set'}${show.system.firmware ? ` - firmware ${show.system.firmware}` : ''}`), { x: M, y: d.y, size: 13, font: d.font, color: rgb(0.3, 0.3, 0.3) });
  d.y -= 40;
  const count = (k: Bus['kind']) => show.buses.filter((b) => b.kind === k).length;
  const facts = [
    ['Event', opts.event || '-'],
    ['Input channels', `${inputs.length}${others.length ? ` + ${others.length} stereo / FX returns` : ''}`],
    ['Patched', `${inputs.filter((c) => c.source).length} of ${inputs.length}`],
    ['Buses', `${count('aux')} aux, ${count('group')} group, ${count('matrix')} matrix, ${count('fx-send')} FX, ${count('main')} main`],
    ['DCAs / mute groups', `${show.dcas.length} / ${show.muteGroups.length}`],
    ['Scenes', String(show.scenes.length)],
    ['Cues', String(show.cues.length)],
    ['Sockets', `${show.sockets.filter((s) => s.direction === 'in').length} in, ${show.sockets.filter((s) => s.direction === 'out').length} out`],
    ['Sample rate', show.system.sampleRate ? `${show.system.sampleRate} Hz` : '-'],
  ];
  for (const [k, v] of facts) {
    d.page.drawText(clean(k), { x: M, y: d.y, size: 10, font: d.bold });
    d.page.drawText(clean(v), { x: M + 130, y: d.y, size: 10, font: d.font });
    d.y -= 15;
  }
  d.y -= 20;
  d.p(`Generated by Songbook on ${new Date().toLocaleString()}${opts.preparedBy ? ` for ${opts.preparedBy}` : ''}.`, { color: rgb(0.4, 0.4, 0.4) });
  if (show.meta.source) d.p(`Model captured from ${show.meta.source.kind} ${show.meta.source.origin} at ${show.meta.source.at}.`, { color: rgb(0.4, 0.4, 0.4) });
  if (show.meta.notes) {
    d.gap(10);
    d.h2('Show notes');
    d.p(show.meta.notes);
  }

  // ---- desk and I/O
  d.newPage();
  d.h1('Desk and I/O');
  d.p(`Desk name: ${show.system.name || '-'}. ${show.system.units.length} unit(s).`);
  for (const u of show.system.units) {
    const ins = show.sockets.filter((s) => s.unitId === u.id && s.direction === 'in');
    const outs = show.sockets.filter((s) => s.unitId === u.id && s.direction === 'out');
    d.h2(`${u.label}${u.model ? ` (${u.model})` : ''} - ${u.role}${u.address ? ` - ${u.address}` : ''}`);
    if (ins.length) {
      d.table(
        ['Input socket', 'Kind', 'Feeds', 'Gain', 'Pad', '48V'],
        ins.map((s) => {
          const p = show.preamps.find((x) => x.socketId === s.id);
          const feeds = show.channels.filter((c) => c.source === s.id || c.sourceRight === s.id).map((c) => c.label).join(', ');
          return [s.label, s.kind, feeds || '-', p?.gainDb !== undefined ? fmtDb(p.gainDb) : '', p?.pad === undefined ? '' : onOff(p.pad), p?.phantom === undefined ? '' : onOff(p.phantom)];
        }),
        [110, 60, 200, 55, 43, 43],
      );
    }
    if (outs.length) {
      d.table(
        ['Output socket', 'Kind', 'Carries'],
        outs.map((s) => {
          const o = show.outputPatch.find((x) => x.socketId === s.id);
          const ref = o?.source.refId ? (bus(o.source.refId)?.label ?? show.channels.find((c) => c.id === o.source.refId)?.label) : undefined;
          return [s.label, s.kind, o ? (ref ?? o.source.label) : '-'];
        }),
        [110, 60, 341],
      );
    }
  }

  // ---- input list
  d.newPage();
  d.h1('Input list');
  d.table(
    ['#', 'Name', 'Source', 'Gain', '48V', 'Fader', 'On', 'Pan', 'Main', 'DCA', 'Mute grp', 'Proc'],
    inputs.map((c) => {
      const p = c.source ? show.preamps.find((x) => x.socketId === c.source) : undefined;
      const proc = [c.strip.hpf?.on ? 'HPF' : '', c.strip.eq?.on ? 'EQ' : '', c.strip.gate?.on ? 'Gate' : '', c.strip.comp?.on ? 'Comp' : '', c.strip.delayMs ? `${c.strip.delayMs} ms` : ''].filter(Boolean).join(' ');
      return [
        String(c.number),
        { swatch: c.color, text: c.label },
        c.source ? socketLabel(show, c.source) : '-',
        p?.gainDb !== undefined ? fmtDb(p.gainDb) : '',
        p?.phantom === undefined ? '' : onOff(p.phantom),
        fmtDb(c.strip.faderDb),
        onOff(c.strip.on),
        fmtPan(c.strip.pan),
        onOff(c.strip.mainAssign),
        c.dcaIds.map((id) => String(show.dcas.find((x) => x.id === id)?.number ?? '')).join(' '),
        c.muteGroupIds.map((id) => String(show.muteGroups.find((x) => x.id === id)?.number ?? '')).join(' '),
        proc,
      ];
    }),
    [22, 82, 92, 40, 28, 42, 24, 30, 30, 36, 40, 45],
    7.5,
  );
  if (others.length) {
    d.h2('Stereo inputs and FX returns');
    d.table(
      ['Kind', '#', 'Name', 'Source', 'Fader', 'On', 'Pan'],
      others.map((c) => [c.kind === 'stereo-input' ? 'ST' : 'FX', String(c.number), { swatch: c.color, text: c.label }, c.source ? socketLabel(show, c.source) : '-', fmtDb(c.strip.faderDb), onOff(c.strip.on), fmtPan(c.strip.pan)]),
      [40, 30, 120, 160, 60, 40, 61],
    );
  }

  // ---- sends
  const sendBuses = show.buses.filter((b) => b.kind === 'aux' || b.kind === 'fx-send' || b.kind === 'group');
  if (opts.includeSends && sendBuses.length && inputs.some((c) => c.sends.length)) {
    d.newPage();
    d.h1('Sends');
    d.p('Levels in dB; a blank cell was not read; "off" is a send at -inf or switched off.', { color: rgb(0.4, 0.4, 0.4) });
    for (let start = 0; start < sendBuses.length; start += 12) {
      const cols = sendBuses.slice(start, start + 12);
      const w0 = 90;
      const cw = (A4.w - 2 * M - w0) / cols.length;
      d.table(
        ['Channel', ...cols.map((b) => b.label)],
        inputs.map((c) => [
          `${c.number} ${c.label}`,
          ...cols.map((b) => {
            const s = c.sends.find((x) => x.busId === b.id);
            if (!s || s.levelDb === undefined) return '';
            if (s.on === false || s.levelDb <= FADER_OFF + 0.5) return 'off';
            return `${fmtDb(s.levelDb).replace(' dB', '')}${s.pre ? ' pre' : ''}`;
          }),
        ]),
        [w0, ...cols.map(() => cw)],
        7,
      );
    }
  }

  // ---- buses
  d.newPage();
  d.h1('Buses and outputs');
  d.table(
    ['Kind', '#', 'Name', 'Stereo', 'Fader', 'On', 'Balance', 'Outputs'],
    show.buses.map((b) => [BUS_KIND_LABEL[b.kind], String(b.number), { swatch: b.color, text: b.label }, b.stereo ? 'yes' : 'mono', fmtDb(b.strip.faderDb), onOff(b.strip.on), fmtPan(b.strip.pan), show.outputPatch.filter((o) => o.source.refId === b.id).map((o) => socketLabel(show, o.socketId)).join(', ') || '-']),
    [55, 25, 110, 45, 50, 30, 50, 146],
  );
  if (show.dcas.length) {
    d.h2('DCAs');
    d.table(
      ['#', 'Name', 'Fader', 'On', 'Members'],
      show.dcas.map((x) => [String(x.number), { swatch: x.color, text: x.label }, fmtDb(x.faderDb), onOff(x.on), show.channels.filter((c) => c.dcaIds.includes(x.id)).map((c) => c.label).join(', ') || '-']),
      [30, 110, 50, 30, 291],
    );
  }
  if (show.muteGroups.length) {
    d.h2('Mute groups');
    d.table(
      ['#', 'Name', 'Active', 'Members'],
      show.muteGroups.map((x) => [String(x.number), x.label, onOff(x.on), show.channels.filter((c) => c.muteGroupIds.includes(x.id)).map((c) => c.label).join(', ') || '-']),
      [30, 110, 45, 326],
    );
  }

  // ---- scenes and cues
  if (opts.includeScenes && (show.scenes.length || show.cues.length)) {
    d.newPage();
    d.h1('Scenes and cues');
    if (show.scenes.length) {
      d.table(['Scene', 'Name', 'Notes'], show.scenes.map((s) => [`${s.bank ?? ''}${s.number ?? '-'}`, s.label, s.notes]), [50, 150, 311]);
    }
    if (show.cues.length) {
      d.h2('Cue list');
      d.table(
        ['Cue', 'Name', 'Recalls', 'Notes'],
        show.cues.map((c) => {
          const step = c.steps.find((s) => s.kind === 'recall-scene');
          const sc = step?.sceneId ? show.scenes.find((s) => s.id === step.sceneId) : undefined;
          return [String(c.number ?? '-'), c.label, sc ? `${sc.bank ?? ''}${sc.number ?? ''} ${sc.label}` : '-', c.notes];
        }),
        [40, 140, 130, 201],
      );
    }
  }

  // ---- notes
  if (show.notes.length) {
    d.newPage();
    d.h1('Import and conversion notes');
    d.table(['Level', 'Where', 'Note'], show.notes.map((n) => [n.level, n.path, n.message]), [50, 120, 341]);
  }

  // ---- history
  if (opts.includeHistory && commits.length) {
    d.newPage();
    d.h1('Version history');
    d.table(['When', 'Who', 'Message', 'Changes'], [...commits].reverse().map((c) => [c.at.replace('T', ' ').replace('Z', ''), c.author ?? '', c.message, String(c.changes)]), [110, 70, 271, 60]);
  }

  return d.doc.save();
}
