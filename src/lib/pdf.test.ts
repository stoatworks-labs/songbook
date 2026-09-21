import { readFileSync } from 'node:fs';

import { PDFDocument } from 'pdf-lib';
import { describe, expect, it } from 'vitest';

import type { Show } from '../types';
import { inputListCsv } from './csv';
import { buildDocument } from './document';
import { THEME_LIST, THEMES } from './doc/theme';
import { clean } from './doc/render-pdf';
import { DEFAULT_DOC_OPTIONS, buildHtml, buildPdf } from './pdf';

const load = (name: string) => JSON.parse(readFileSync(new URL(`../../public/demo/${name}.json`, import.meta.url), 'utf8')) as Show;

const commits = (show: Show) => [
  {
    id: 'a',
    at: '2026-09-19T20:00:00Z',
    message: 'Demo',
    hash: 'x',
    changes: 0,
    summary: { id: show.id, name: show.meta.name, platform: show.platform, model: show.system.model, firmware: '', modified: '', inputs: 0, outputs: 0, channels: 0, namedChannels: 0, auxes: 0, groups: 0, matrices: 0, fx: 0, dcas: 0, muteGroups: 0, scenes: 0, cues: 0, notesDropped: 0 },
  },
];

const withProduction = (show: Show): Show => ({
  ...show,
  meta: { ...show.meta, production: { event: 'Spring Conference', client: 'Northwind', company: 'Stage Co', venue: 'The Round', date: '3 October 2026', operator: 'A. Engineer', contact: 'a@example.com' } },
});

describe('documents', () => {
  it('builds a PDF for both demo shows', async () => {
    for (const name of ['sq5-band', 'dm3-corporate']) {
      const show = load(name);
      const bytes = await buildPdf(show, commits(show), DEFAULT_DOC_OPTIONS);
      expect(bytes.length).toBeGreaterThan(10_000);
      expect(String.fromCharCode(...bytes.subarray(0, 5))).toBe('%PDF-');
      const doc = await PDFDocument.load(bytes);
      // Cover, desk/IO, flow, input list, levels, sends, buses, scenes, glossary, notes/history.
      expect(doc.getPageCount()).toBeGreaterThanOrEqual(9);
    }
  });

  it('renders every theme and both papers', async () => {
    const show = load('sq5-band');
    for (const t of THEME_LIST) {
      const bytes = await buildPdf(show, [], { ...DEFAULT_DOC_OPTIONS, theme: t.id });
      expect(String.fromCharCode(...bytes.subarray(0, 5))).toBe('%PDF-');
      const html = buildHtml(show, [], { ...DEFAULT_DOC_OPTIONS, theme: t.id });
      expect(html).toContain(`--accent:${t.accent}`);
    }
    const letter = await PDFDocument.load(await buildPdf(show, [], { ...DEFAULT_DOC_OPTIONS, paper: 'letter' }));
    expect(Math.round(letter.getPage(0).getWidth())).toBe(612);
    const a4 = await PDFDocument.load(await buildPdf(show, [], { ...DEFAULT_DOC_OPTIONS, paper: 'a4' }));
    expect(Math.round(a4.getPage(0).getWidth())).toBe(595);
  });

  it('puts the production details on the cover of both formats', async () => {
    const show = withProduction(load('sq5-band'));
    const html = buildHtml(show, [], DEFAULT_DOC_OPTIONS);
    for (const v of ['Spring Conference', 'Northwind', 'Stage Co', 'The Round', '3 October 2026', 'A. Engineer']) expect(html).toContain(v);
    const doc = buildDocument(show, [], DEFAULT_DOC_OPTIONS);
    const cover = doc.blocks[0];
    expect(cover.kind).toBe('cover');
    if (cover.kind === 'cover') {
      expect(cover.title).toBe('Spring Conference');
      expect(cover.production.map(([k]) => k)).toContain('Venue');
    }
    expect(doc.title.startsWith('Spring Conference')).toBe(true);
  });

  it('writes a self-contained HTML page with a table of contents and diagrams', () => {
    const show = load('sq5-band');
    const html = buildHtml(show, commits(show), DEFAULT_DOC_OPTIONS);
    expect(html.startsWith('<!doctype html>')).toBe(true);
    expect(html).not.toMatch(/<(script|link|img)[^>]+\b(src|href)="(https?:)?\/\//);
    expect(html).toContain('<svg');
    expect(html).toContain('id="q"');
    for (const heading of ['Input list', 'Send matrix', 'Buses and outputs', 'Signal flow']) expect(html).toContain(`>${heading}</h1>`);
    // The nav links to each section that exists.
    const ids = [...html.matchAll(/<section class="s" id="([^"]+)"/g)].map((m) => m[1]);
    expect(ids.length).toBeGreaterThan(5);
    for (const id of ids) expect(html).toContain(`href="#${id}"`);
  });

  it('honours the section switches', () => {
    const show = load('sq5-band');
    const off = buildHtml(show, [], { ...DEFAULT_DOC_OPTIONS, includeSends: false, includeScenes: false, includeGlossary: false, includeProcessing: false });
    expect(off).not.toContain('>Send matrix</h1>');
    expect(off).not.toContain('>Scenes and cues</h1>');
    expect(off).not.toContain('>What the settings mean</h1>');
    expect(off).not.toContain('>Levels and processing</h1>');
    expect(buildHtml(show, [], DEFAULT_DOC_OPTIONS)).toContain('>Send matrix</h1>');
  });

  it('escapes HTML in show data', () => {
    const show = load('sq5-band');
    const evil: Show = { ...show, meta: { ...show.meta, name: '<script>alert(1)</script>', notes: 'a & b' } };
    const html = buildHtml(evil, [], DEFAULT_DOC_OPTIONS);
    expect(html).not.toContain('<script>alert(1)</script>');
    expect(html).toContain('&lt;script&gt;');
    expect(html).toContain('a &amp; b');
  });

  it('maps characters pdf-lib cannot encode', () => {
    expect(clean('−∞ dB · 1920×1080 → out')).toBe('-inf dB - 1920x1080 -> out');
    expect(clean('naïve – dash')).toBe('naïve – dash');
  });

  it('the ink theme uses no coloured fills', () => {
    expect(THEMES.ink.ink).toBe(true);
    const html = buildHtml(load('sq5-band'), [], { ...DEFAULT_DOC_OPTIONS, theme: 'ink' });
    expect(html).toContain('class="ink ');
    // No heat fills are emitted in ink mode.
    expect(html).not.toMatch(/<td[^>]*style="background:#/);
  });

  it('writes the input list as CSV', () => {
    const csv = inputListCsv(load('sq5-band'));
    const lines = csv.trim().split('\n');
    expect(lines[0].startsWith('#,Kind,Name')).toBe(true);
    expect(lines.length).toBe(1 + load('sq5-band').channels.length);
    expect(lines[1]).toContain('Kick In');
  });
});
