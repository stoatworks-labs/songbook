/** The input list as CSV, for a spreadsheet or a stage plot tool. */
import { fmtDb, fmtPan, socketLabel, type Show } from '../types';

const cell = (v: unknown) => {
  const s = String(v ?? '');
  return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
};

export function inputListCsv(show: Show): string {
  const rows: unknown[][] = [['#', 'Kind', 'Name', 'Colour', 'Source', 'Gain', 'Pad', '48V', 'Fader', 'On', 'Pan', 'Main', 'DCAs', 'Mute groups', 'HPF', 'EQ', 'Gate', 'Comp']];
  for (const c of show.channels) {
    const p = c.source ? show.preamps.find((x) => x.socketId === c.source) : undefined;
    rows.push([
      c.number,
      c.kind,
      c.label,
      c.color ?? '',
      c.source ? socketLabel(show, c.source) : '',
      p?.gainDb !== undefined ? fmtDb(p.gainDb) : '',
      p?.pad === undefined ? '' : p.pad ? 'on' : 'off',
      p?.phantom === undefined ? '' : p.phantom ? 'on' : 'off',
      c.strip.faderDb === undefined ? '' : fmtDb(c.strip.faderDb),
      c.strip.on === undefined ? '' : c.strip.on ? 'on' : 'off',
      fmtPan(c.strip.pan).replace('—', ''),
      c.strip.mainAssign === undefined ? '' : c.strip.mainAssign ? 'on' : 'off',
      c.dcaIds.map((id) => show.dcas.find((d) => d.id === id)?.number ?? '').join(' '),
      c.muteGroupIds.map((id) => show.muteGroups.find((m) => m.id === id)?.number ?? '').join(' '),
      c.strip.hpf ? (c.strip.hpf.on ? 'on' : 'off') : '',
      c.strip.eq ? (c.strip.eq.on ? 'on' : 'off') : '',
      c.strip.gate ? (c.strip.gate.on ? 'on' : 'off') : '',
      c.strip.comp ? (c.strip.comp.on ? 'on' : 'off') : '',
    ]);
  }
  return rows.map((r) => r.map(cell).join(',')).join('\n') + '\n';
}
