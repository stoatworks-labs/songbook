/**
 * The show documentation as a `Document`: cover with the production
 * details, the desk and its units drawn as a socket map, the signal flow,
 * the input list, fader and processing overviews, the send matrix as a
 * heat grid, buses and outputs, DCA and mute-group membership as a matrix,
 * scenes and cues, a glossary, the import notes and the version history.
 * `pdf.ts` renders it as a PDF or as an HTML page.
 */
import { flowDiagram, legend, type FlowLink, type FlowNode } from './doc/diagrams';
import { DocBuilder, fitText, type Diagram, type DiagramItem, type Document, type Tile } from './doc/ir';
import { contentWidth, type PaperId, type ThemeId } from './doc/theme';
import { GLOSSARY } from './glossary';
import { BUS_KIND_LABEL, FADER_OFF, PLATFORM_LABEL, fmtDb, fmtPan, socketLabel, type Bus, type Channel, type Commit, type Show } from '../types';

export interface DocOptions {
  includeSends: boolean;
  includeScenes: boolean;
  includeHistory: boolean;
  includeProcessing: boolean;
  includeGlossary: boolean;
  /** "Prepared for" on the cover. */
  preparedBy: string;
  /** "Prepared by" — the library's author from Settings. */
  author?: string;
  /** Kept for the older option shape; the production details own it now. */
  event: string;
  theme?: ThemeId;
  paper?: PaperId;
}

export const DEFAULT_DOC_OPTIONS: DocOptions = {
  includeSends: true,
  includeScenes: true,
  includeHistory: true,
  includeProcessing: true,
  includeGlossary: true,
  preparedBy: '',
  event: '',
  theme: 'light',
  paper: 'a4',
};

const onOff = (v?: boolean) => (v === undefined ? '' : v ? 'on' : 'off');

const fmtDate = (iso: string) => {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString();
};

/** Desk colour name to a theme-independent hex (the colours are the desk's). */
const DESK_COLORS: Record<string, string> = {
  off: '#8c93a1',
  red: '#e0443e',
  green: '#3bb45c',
  yellow: '#d9ad1f',
  blue: '#3b7be0',
  purple: '#8f5bd6',
  cyan: '#2fa8bd',
  white: '#9aa3b2',
  orange: '#e0821f',
  pink: '#e05a9e',
};
const deskColor = (c?: string) => DESK_COLORS[c ?? 'off'] ?? DESK_COLORS.off;
/** The desk colour as a literal, or nothing when the strip carries no colour. */
const swatch = (c?: string) => (c && c !== 'off' ? deskColor(c) : undefined);

/**
 * A channel the show actually uses: patched, in the main, or with a send up.
 * The overviews list these; the input list still shows every channel.
 */
export const isLive = (c: Channel): boolean =>
  Boolean(c.source) || c.strip.mainAssign === true || c.sends.some((x) => x.on !== false && x.levelDb !== undefined && x.levelDb > FADER_OFF + 0.5);

/** −inf..+10 dB to 0..1, for the send matrix heat. */
function faderHeat(db?: number): number | undefined {
  if (db === undefined) return undefined;
  if (db <= FADER_OFF + 0.5) return 0;
  return Math.max(0, Math.min(1, (db + 40) / 50));
}

/** Every unit as a row of sockets, coloured by whether the show uses them. */
function socketMap(show: Show, cw: number): Diagram {
  const items: DiagramItem[] = [];
  const labelW = 140;
  const sq = 10;
  const gap = 3;
  const perRow = Math.max(1, Math.floor((cw - labelW - 6) / (sq + gap)));
  const leg = legend(
    [
      { label: 'input, feeding a channel', color: 'palette:0' },
      { label: 'output, carrying a mix', color: 'palette:1' },
      { label: 'not patched', color: 'rule' },
    ],
    cw,
  );
  items.push(...leg.items);
  let y = leg.h + 6;
  for (const u of show.system.units) {
    for (const dir of ['in', 'out'] as const) {
      const sockets = show.sockets.filter((s) => s.unitId === u.id && s.direction === dir);
      if (!sockets.length) continue;
      const rows = Math.ceil(sockets.length / perRow);
      const rowH = rows * (sq + gap) + 6;
      items.push({ t: 'rect', x: 0, y, w: cw, h: rowH, stroke: 'rule', sw: 0.4 });
      items.push({ t: 'text', x: 4, y: y + 9, text: fitText(u.label, 6.5, labelW - 8, true), size: 6.5, bold: true, color: 'text' });
      items.push({ t: 'text', x: 4, y: y + 17, text: `${sockets.length} ${dir === 'in' ? 'inputs' : 'outputs'}${u.model ? ` · ${u.model}` : ''}`, size: 5.5, color: 'muted' });
      sockets.forEach((s, i) => {
        const cx = labelW + (i % perRow) * (sq + gap);
        const cy = y + 3 + Math.floor(i / perRow) * (sq + gap);
        const used =
          dir === 'in' ? show.channels.some((c) => c.source === s.id || c.sourceRight === s.id) : show.outputPatch.some((o) => o.socketId === s.id);
        const fill = used ? (dir === 'in' ? 'palette:0' : 'palette:1') : undefined;
        items.push({ t: 'rect', x: cx, y: cy, w: sq, h: sq, fill, stroke: fill ? undefined : 'rule', sw: 0.5, r: 1.5 });
        items.push({ t: 'text', x: cx + sq / 2, y: cy + sq - 2.6, text: String(s.index), size: 4.6, anchor: 'middle', color: fill ? '#ffffff' : 'muted', bold: Boolean(fill) });
      });
      y += rowH + 2;
    }
  }
  return { w: cw, h: Math.max(y, leg.h + 8), items };
}

/** Input sockets → channels → buses → output sockets. */
function signalFlow(show: Show, cw: number): Diagram {
  const inputs = show.channels.filter((c) => c.kind === 'input');
  const sockets: FlowNode[] = show.sockets
    .filter((s) => s.direction === 'in' && show.channels.some((c) => c.source === s.id || c.sourceRight === s.id))
    .map((s) => ({ id: s.id, label: socketLabel(show, s.id), sub: s.kind, color: 'palette:0' }));
  // Channels that do nothing would fill the column with empty strips; they
  // are in the input list instead.
  const chans: FlowNode[] = inputs.filter(isLive).map((c) => ({ id: c.id, label: `${c.number} ${c.label}`, sub: c.source ? socketLabel(show, c.source) : 'not patched', color: swatch(c.color) ?? 'palette:3' }));
  const buses: FlowNode[] = show.buses.map((b) => ({ id: b.id, label: b.label, sub: BUS_KIND_LABEL[b.kind], color: swatch(b.color) ?? (b.kind === 'main' ? 'palette:1' : 'palette:2') }));
  const outs: FlowNode[] = show.outputPatch.map((o) => ({ id: `out:${o.socketId}`, label: socketLabel(show, o.socketId), sub: o.source.label, color: 'palette:1' }));
  const links: FlowLink[] = [];
  for (const c of inputs.filter(isLive)) {
    if (c.source) links.push({ from: c.source, to: c.id, color: 'palette:0' });
    for (const s of c.sends) {
      if (s.on === false || s.levelDb === undefined || s.levelDb <= FADER_OFF + 0.5) continue;
      links.push({ from: c.id, to: s.busId, color: swatch(c.color) ?? 'palette:3', weight: 0.8 });
    }
    if (c.strip.mainAssign) for (const b of show.buses.filter((x) => x.kind === 'main')) links.push({ from: c.id, to: b.id, color: 'palette:1' });
  }
  for (const o of show.outputPatch) if (o.source.refId) links.push({ from: o.source.refId, to: `out:${o.socketId}`, color: 'palette:1' });
  return flowDiagram(
    [
      { title: 'Input sockets', nodes: sockets },
      { title: 'Channels', nodes: chans },
      { title: 'Buses', nodes: buses },
      { title: 'Output sockets', nodes: outs },
    ],
    links,
    cw,
  );
}

/** A bar per channel at its fader level, so the balance is visible at a glance. */
function faderChart(channels: Channel[], cw: number): Diagram {
  const items: DiagramItem[] = [];
  const labelW = 88;
  const rowH = 9;
  const min = -40;
  const max = 10;
  const barW = cw - labelW - 34;
  const zero = labelW + (barW * (0 - min)) / (max - min);
  items.push({ t: 'line', x1: zero, y1: 8, x2: zero, y2: 12 + channels.length * rowH, stroke: 'rule', sw: 0.5, dash: [2, 2] });
  items.push({ t: 'text', x: zero, y: 6, text: '0 dB', size: 5.5, anchor: 'middle', color: 'muted' });
  items.push({ t: 'text', x: labelW, y: 6, text: `${min}`, size: 5.5, color: 'muted' });
  channels.forEach((c, i) => {
    const y = 10 + i * rowH;
    const db = c.strip.faderDb;
    items.push({ t: 'text', x: 0, y: y + 6, text: fitText(`${c.number} ${c.label}`, 5.8, labelW - 4), size: 5.8, color: 'text' });
    if (db === undefined) {
      items.push({ t: 'text', x: labelW, y: y + 6, text: 'not read', size: 5.5, color: 'muted' });
      return;
    }
    const off = db <= FADER_OFF + 0.5;
    const v = Math.max(min, Math.min(max, db));
    const x = labelW + (barW * (Math.min(v, 0) - min)) / (max - min);
    const w = Math.max(2, (barW * (Math.max(v, 0) - Math.min(v, 0))) / (max - min));
    const color = off ? 'muted' : swatch(c.color) ?? 'accent';
    if (off) items.push({ t: 'text', x: labelW, y: y + 6, text: '−∞', size: 5.5, color: 'muted' });
    else items.push({ t: 'rect', x: Math.min(x, zero), y: y + 1.5, w: v < 0 ? Math.max(2, zero - x) : w, h: rowH - 4, fill: color, opacity: c.strip.on === false ? 0.3 : 0.85, r: 1 });
    items.push({ t: 'text', x: cw, y: y + 6, text: `${fmtDb(db)}${c.strip.on === false ? ' (muted)' : ''}`, size: 5.5, anchor: 'end', color: 'muted' });
  });
  return { w: cw, h: 14 + channels.length * rowH, items };
}

export function buildDocument(show: Show, commits: Commit[], opts: DocOptions): Document {
  const cw = contentWidth(opts.paper ?? 'a4');
  const d = new DocBuilder(cw);
  const inputs = show.channels.filter((c) => c.kind === 'input');
  const others = show.channels.filter((c) => c.kind !== 'input');
  // The overviews (levels, processing, sends, membership) only carry the
  // channels the show uses; a desk with 48 strips and a 22-piece band would
  // otherwise print 26 empty rows per table.
  const used = inputs.filter(isLive).length ? inputs.filter(isLive) : inputs;
  const bus = (id: string) => show.buses.find((b) => b.id === id);
  const count = (k: Bus['kind']) => show.buses.filter((b) => b.kind === k).length;
  const prod = show.meta.production ?? {};
  const platform = `${PLATFORM_LABEL[show.platform]} – ${show.system.model || 'model not set'}${show.system.firmware ? ` – firmware ${show.system.firmware}` : ''}`;
  const title = prod.event?.trim() || opts.event.trim() || show.meta.name;

  // ---- cover
  const production: [string, string][] = [];
  if (title !== show.meta.name) production.push(['Show file', show.meta.name]);
  if (prod.client) production.push(['Client', prod.client]);
  if (prod.company) production.push(['Production company', prod.company]);
  if (prod.venue) production.push(['Venue', prod.venue]);
  if (prod.date) production.push(['Date', prod.date]);
  if (prod.operator) production.push(['Engineer', prod.operator]);
  if (prod.contact) production.push(['Contact', prod.contact]);
  const tiles: Tile[] = [
    { label: 'input channels', value: String(inputs.length), hint: others.length ? `+ ${others.length} ST/FX` : undefined },
    { label: 'patched', value: `${inputs.filter((c) => c.source).length}`, hint: `of ${inputs.length}` },
    { label: 'aux / group', value: `${count('aux')} / ${count('group')}` },
    { label: 'matrix / FX', value: `${count('matrix')} / ${count('fx-send')}` },
    { label: 'DCAs', value: String(show.dcas.length) },
    { label: 'mute groups', value: String(show.muteGroups.length) },
    { label: 'scenes', value: String(show.scenes.length), hint: show.cues.length ? `${show.cues.length} cues` : undefined },
    { label: 'sample rate', value: show.system.sampleRate ? `${show.system.sampleRate / 1000} kHz` : '–' },
  ];
  const lines = [`Generated by Songbook on ${new Date().toLocaleString()}${opts.preparedBy ? ` for ${opts.preparedBy}` : ''}${opts.author ? `, prepared by ${opts.author}` : ''}.`];
  if (show.meta.source) lines.push(`Model captured from ${show.meta.source.kind} ${show.meta.source.origin} at ${fmtDate(show.meta.source.at)}.`);
  d.cover({ title, subtitle: title === show.meta.name ? platform : `${show.meta.name} · ${platform}`, production, tiles, lines, notes: show.meta.notes || undefined });

  // ---- desk and I/O
  d.pagebreak();
  d.h1('Desk and I/O');
  d.p(`Desk name: ${show.system.name || '–'}. ${show.system.units.length} unit(s)${show.system.clockSource ? `, clock ${show.system.clockSource}` : ''}.`);
  if (show.sockets.length) d.diagram(socketMap(show, cw), 'Each square is a socket, numbered as on the unit; colour says whether the show patches it.');
  for (const u of show.system.units) {
    const ins = show.sockets.filter((s) => s.unitId === u.id && s.direction === 'in');
    const outs = show.sockets.filter((s) => s.unitId === u.id && s.direction === 'out');
    d.h2(`${u.label}${u.model ? ` (${u.model})` : ''} – ${u.role}${u.address ? ` – ${u.address}` : ''}`);
    if (ins.length) {
      d.table(
        ['Input socket', 'Kind', 'Feeds', 'Gain', 'Pad', '48V'],
        ins.map((s) => {
          const p = show.preamps.find((x) => x.socketId === s.id);
          const feeds = show.channels.filter((c) => c.source === s.id || c.sourceRight === s.id);
          return [
            s.label,
            s.kind,
            feeds.length ? { text: feeds.map((c) => c.label).join(', '), swatch: swatch(feeds[0].color) } : { text: '–', muted: true },
            { text: p?.gainDb !== undefined ? fmtDb(p.gainDb) : '', align: 'r' as const },
            p?.pad === undefined ? '' : { text: onOff(p.pad), dot: p.pad ? 'warn' : undefined },
            p?.phantom === undefined ? '' : { text: onOff(p.phantom), dot: p.phantom ? 'bad' : undefined },
          ];
        }),
        { widths: [110, 60, 200, 55, 43, 43], align: ['l', 'l', 'l', 'r', 'l', 'l'] },
      );
    }
    if (outs.length) {
      d.table(
        ['Output socket', 'Kind', 'Carries'],
        outs.map((s) => {
          const o = show.outputPatch.find((x) => x.socketId === s.id);
          const b = o?.source.refId ? bus(o.source.refId) : undefined;
          const ref = b?.label ?? (o?.source.refId ? show.channels.find((c) => c.id === o.source.refId)?.label : undefined);
          return [s.label, s.kind, o ? { text: ref ?? o.source.label, swatch: swatch(b?.color) } : { text: '–', muted: true }];
        }),
        { widths: [110, 60, 341] },
      );
    }
  }

  // ---- signal flow
  if (inputs.length) {
    d.pagebreak();
    d.h1('Signal flow');
    d.p('Where every patched socket goes: into its channel, on through the sends that are up and the main assignment, and out of the sockets that carry each mix.', true);
    d.diagram(signalFlow(show, cw));
  }

  // ---- input list
  d.pagebreak();
  d.h1('Input list');
  d.table(
    ['#', 'Name', 'Source', 'Gain', '48V', 'Fader', 'On', 'Pan', 'Main', 'DCA', 'Mute grp', 'Proc'],
    inputs.map((c) => {
      const p = c.source ? show.preamps.find((x) => x.socketId === c.source) : undefined;
      const proc = [c.strip.hpf?.on ? 'HPF' : '', c.strip.eq?.on ? 'EQ' : '', c.strip.gate?.on ? 'Gate' : '', c.strip.comp?.on ? 'Comp' : '', c.strip.delayMs ? `${c.strip.delayMs} ms` : ''].filter(Boolean).join(' ');
      return [
        { text: String(c.number), align: 'r' as const },
        { text: c.label, swatch: deskColor(c.color) },
        c.source ? socketLabel(show, c.source) : { text: '–', muted: true },
        { text: p?.gainDb !== undefined ? fmtDb(p.gainDb) : '', align: 'r' as const },
        p?.phantom === undefined ? '' : { text: onOff(p.phantom), dot: p.phantom ? 'bad' : undefined },
        { text: fmtDb(c.strip.faderDb), align: 'r' as const },
        { text: onOff(c.strip.on), dot: c.strip.on === false ? 'muted' : c.strip.on ? 'good' : undefined },
        { text: fmtPan(c.strip.pan), align: 'c' as const },
        onOff(c.strip.mainAssign),
        c.dcaIds.map((id) => String(show.dcas.find((x) => x.id === id)?.number ?? '')).join(' '),
        c.muteGroupIds.map((id) => String(show.muteGroups.find((x) => x.id === id)?.number ?? '')).join(' '),
        proc,
      ];
    }),
    { widths: [22, 82, 92, 40, 28, 42, 24, 30, 30, 36, 40, 45], size: 'tiny', align: ['r', 'l', 'l', 'r', 'l', 'r', 'l', 'c', 'l', 'l', 'l', 'l'] },
  );
  if (others.length) {
    d.h2('Stereo inputs and FX returns');
    d.table(
      ['Kind', '#', 'Name', 'Source', 'Fader', 'On', 'Pan'],
      others.map((c) => [
        c.kind === 'stereo-input' ? 'ST' : 'FX',
        { text: String(c.number), align: 'r' as const },
        { text: c.label, swatch: deskColor(c.color) },
        c.source ? socketLabel(show, c.source) : { text: '–', muted: true },
        { text: fmtDb(c.strip.faderDb), align: 'r' as const },
        onOff(c.strip.on),
        { text: fmtPan(c.strip.pan), align: 'c' as const },
      ]),
      { widths: [40, 30, 120, 160, 60, 40, 61], align: ['l', 'r', 'l', 'l', 'r', 'l', 'c'] },
    );
  }

  // ---- fader and processing overview
  if (opts.includeProcessing && inputs.length) {
    d.pagebreak();
    d.h1('Levels and processing');
    d.p(`Every channel the show uses, at its fader level; a dimmed bar is a muted channel.${used.length < inputs.length ? ` ${inputs.length - used.length} unused channel(s) are left out.` : ''}`, true);
    d.diagram(faderChart(used, cw));
    const procs: [string, (c: Channel) => boolean | undefined][] = [
      ['HPF', (c) => c.strip.hpf?.on],
      ['EQ', (c) => c.strip.eq?.on],
      ['Gate', (c) => c.strip.gate?.on],
      ['Comp', (c) => c.strip.comp?.on],
      ['Delay', (c) => (c.strip.delayMs ? c.strip.delayMs > 0 : undefined)],
      ['Polarity', (c) => c.strip.polarity],
      ['Insert', (c) => c.strip.insertOn],
    ];
    const cols = procs.filter(([, f]) => used.some((c) => f(c) !== undefined));
    if (cols.length) {
      d.h2('Which channels use what');
      d.p('A dot is on; an empty cell is off or not read. The last row counts the channels.', true);
      d.table(
        ['Channel', ...cols.map(([n]) => n)],
        [
          ...used.map((c) => [
            { text: `${c.number} ${c.label}`, swatch: deskColor(c.color) },
            ...cols.map(([, f]) => (f(c) ? { text: '', dot: 'accent', align: 'c' as const } : { text: '', align: 'c' as const })),
          ]),
          [{ text: 'total', bold: true }, ...cols.map(([, f]) => ({ text: String(inputs.filter((c) => f(c)).length), bold: true, align: 'c' as const }))],
        ],
        { widths: [140, ...cols.map(() => Math.max(28, 371 / cols.length))], size: 'small', align: ['l', ...cols.map(() => 'c' as const)] },
      );
    }
  }

  // ---- sends
  const sendBuses = show.buses.filter((b) => b.kind === 'aux' || b.kind === 'fx-send' || b.kind === 'group');
  if (opts.includeSends && sendBuses.length && inputs.some((c) => c.sends.length)) {
    d.pagebreak();
    d.h1('Send matrix');
    const sendRows = inputs.filter((c) => c.sends.length);
    d.p(`Levels in dB. The darker the cell the hotter the send; "off" is a send at −∞ or switched off, and a blank cell was not read. A send marked "pre" ignores the channel fader.${sendRows.length < inputs.length ? ` Channels with no sends read are left out.` : ''}`, true);
    for (let start = 0; start < sendBuses.length; start += 12) {
      const cols = sendBuses.slice(start, start + 12);
      if (sendBuses.length > 12) d.h2(`${cols[0].label} – ${cols[cols.length - 1].label}`);
      d.table(
        ['Channel', ...cols.map((b) => b.label)],
        sendRows.map((c) => [
          { text: `${c.number} ${c.label}`, swatch: deskColor(c.color) },
          ...cols.map((b) => {
            const s = c.sends.find((x) => x.busId === b.id);
            if (!s || s.levelDb === undefined) return { text: '', align: 'c' as const };
            if (s.on === false || s.levelDb <= FADER_OFF + 0.5) return { text: 'off', muted: true, align: 'c' as const };
            return { text: `${fmtDb(s.levelDb).replace(' dB', '')}${s.pre ? ' pre' : ''}`, heat: faderHeat(s.levelDb), align: 'c' as const };
          }),
        ]),
        {
          widths: [90, ...cols.map(() => (cw - 90) / cols.length)],
          size: 'tiny',
          align: ['l', ...cols.map(() => 'c' as const)],
          headerSwatches: [undefined, ...cols.map((b) => deskColor(b.color))],
        },
      );
    }
  }

  // ---- buses
  d.pagebreak();
  d.h1('Buses and outputs');
  d.table(
    ['Kind', '#', 'Name', 'Stereo', 'Fader', 'On', 'Balance', 'Outputs'],
    show.buses.map((b) => [
      BUS_KIND_LABEL[b.kind],
      { text: String(b.number), align: 'r' as const },
      { text: b.label, swatch: deskColor(b.color) },
      b.stereo ? 'yes' : 'mono',
      { text: fmtDb(b.strip.faderDb), align: 'r' as const },
      onOff(b.strip.on),
      { text: fmtPan(b.strip.pan), align: 'c' as const },
      show.outputPatch
        .filter((o) => o.source.refId === b.id)
        .map((o) => socketLabel(show, o.socketId))
        .join(', ') || { text: '–', muted: true },
    ]),
    { widths: [55, 25, 110, 45, 50, 30, 50, 146], align: ['l', 'r', 'l', 'l', 'r', 'l', 'c', 'l'] },
  );
  if (show.dcas.length) {
    d.h2('DCAs');
    d.table(
      ['#', 'Name', 'Fader', 'On', 'Members'],
      show.dcas.map((x) => [
        { text: String(x.number), align: 'r' as const },
        { text: x.label, swatch: deskColor(x.color) },
        { text: fmtDb(x.faderDb), align: 'r' as const },
        onOff(x.on),
        show.channels.filter((c) => c.dcaIds.includes(x.id)).map((c) => c.label).join(', ') || { text: '–', muted: true },
      ]),
      { widths: [30, 110, 50, 30, 291], align: ['r', 'l', 'r', 'l', 'l'] },
    );
  }
  if (show.muteGroups.length) {
    d.h2('Mute groups');
    d.table(
      ['#', 'Name', 'Active', 'Members'],
      show.muteGroups.map((x) => [
        { text: String(x.number), align: 'r' as const },
        x.label,
        onOff(x.on),
        show.channels.filter((c) => c.muteGroupIds.includes(x.id)).map((c) => c.label).join(', ') || { text: '–', muted: true },
      ]),
      { widths: [30, 110, 45, 326], align: ['r', 'l', 'l', 'l'] },
    );
  }
  if ((show.dcas.length || show.muteGroups.length) && inputs.length) {
    d.h2('Who belongs to what');
    d.p('Only channels that are in a DCA or a mute group.', true);
    const cols = [...show.dcas.map((x) => ({ id: x.id, label: `DCA ${x.number}`, color: deskColor(x.color), dca: true })), ...show.muteGroups.map((x) => ({ id: x.id, label: `MG ${x.number}`, color: DESK_COLORS.off, dca: false }))];
    d.table(
      ['Channel', ...cols.map((c) => c.label)],
      show.channels
        .filter((c) => c.dcaIds.length || c.muteGroupIds.length)
        .map((c) => [
          { text: `${c.number} ${c.label}`, swatch: deskColor(c.color) },
          ...cols.map((g) => ((g.dca ? c.dcaIds : c.muteGroupIds).includes(g.id) ? { text: '', dot: g.color, align: 'c' as const } : { text: '', align: 'c' as const })),
        ]),
      { widths: [120, ...cols.map(() => Math.max(24, (cw - 120) / Math.max(1, cols.length)))], size: 'tiny', align: ['l', ...cols.map(() => 'c' as const)] },
    );
  }

  // ---- scenes and cues
  if (opts.includeScenes && (show.scenes.length || show.cues.length)) {
    d.pagebreak();
    d.h1('Scenes and cues');
    if (show.scenes.length) {
      d.table(
        ['Scene', 'Name', 'Recall filters', 'Notes'],
        show.scenes.map((s) => [`${s.bank ?? ''}${s.number ?? '–'}`, s.label, s.filters.join(', ') || { text: 'all', muted: true }, s.notes]),
        { widths: [50, 150, 120, 191] },
      );
    }
    if (show.cues.length) {
      d.h2('Cue list');
      d.table(
        ['Cue', 'Name', 'Recalls', 'Notes'],
        show.cues.map((c) => {
          const step = c.steps.find((s) => s.kind === 'recall-scene');
          const sc = step?.sceneId ? show.scenes.find((s) => s.id === step.sceneId) : undefined;
          return [{ text: String(c.number ?? '–'), align: 'r' as const }, c.label, sc ? `${sc.bank ?? ''}${sc.number ?? ''} ${sc.label}` : { text: '–', muted: true }, c.notes];
        }),
        { widths: [40, 140, 130, 201], align: ['r', 'l', 'l', 'l'] },
      );
    }
  }

  // ---- glossary
  if (opts.includeGlossary) {
    d.pagebreak();
    d.h1('What the settings mean');
    d.glossary(GLOSSARY.filter((g) => !g.platforms || g.platforms.includes(show.platform)).map((g) => ({ term: g.term, text: g.text })));
  }

  // ---- notes
  if (show.notes.length) {
    d.pagebreak();
    d.h1('Import and conversion notes');
    d.table(
      ['Level', 'Where', 'Note'],
      show.notes.map((n) => [{ text: n.level, dot: n.level === 'dropped' ? 'bad' : n.level === 'adapted' ? 'warn' : 'muted' }, n.path, n.message]),
      { widths: [50, 120, 341] },
    );
  }

  // ---- history
  if (opts.includeHistory && commits.length) {
    if (!show.notes.length) d.pagebreak();
    d.h1('Version history');
    d.table(
      ['When', 'Who', 'Message', 'Changes'],
      [...commits].reverse().map((c) => [c.at.replace('T', ' ').replace('Z', ''), c.author ?? '', c.message, { text: String(c.changes), align: 'r' as const }]),
      { widths: [110, 70, 271, 60], align: ['l', 'l', 'l', 'r'] },
    );
  }

  return { title: `${title} – ${PLATFORM_LABEL[show.platform]} ${show.system.model}`.trim(), subtitle: platform, producer: 'Songbook', blocks: d.blocks };
}
