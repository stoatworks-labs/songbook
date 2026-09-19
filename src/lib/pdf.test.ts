import { readFileSync } from 'node:fs';

import { describe, expect, it } from 'vitest';

import type { Show } from '../types';
import { inputListCsv } from './csv';
import { buildPdf } from './pdf';

const load = (name: string) => JSON.parse(readFileSync(new URL(`../../public/demo/${name}.json`, import.meta.url), 'utf8')) as Show;

describe('documents', () => {
  it('builds a PDF for both demo shows', async () => {
    for (const name of ['sq5-band', 'dm3-corporate']) {
      const show = load(name);
      const bytes = await buildPdf(show, [{ id: 'a', at: '2026-09-19T20:00:00Z', message: 'Demo', hash: 'x', changes: 0, summary: { id: show.id, name: show.meta.name, platform: show.platform, model: show.system.model, firmware: '', modified: '', inputs: 0, outputs: 0, channels: 0, namedChannels: 0, auxes: 0, groups: 0, matrices: 0, fx: 0, dcas: 0, muteGroups: 0, scenes: 0, cues: 0, notesDropped: 0 } }], { includeSends: true, includeScenes: true, includeHistory: true, preparedBy: 'test', event: 'Test' });
      expect(bytes.length).toBeGreaterThan(10_000);
      expect(String.fromCharCode(...bytes.subarray(0, 5))).toBe('%PDF-');
    }
  });

  it('writes the input list as CSV', () => {
    const csv = inputListCsv(load('sq5-band'));
    const lines = csv.trim().split('\n');
    expect(lines[0].startsWith('#,Kind,Name')).toBe(true);
    expect(lines.length).toBe(1 + load('sq5-band').channels.length);
    expect(lines[1]).toContain('Kick In');
  });
});
