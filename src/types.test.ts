import { describe, expect, it } from 'vitest';

import { fmtDb, fmtPan, FADER_OFF } from './types';

describe('formatting', () => {
  it('formats levels and pans', () => {
    expect(fmtDb(FADER_OFF)).toBe('−∞');
    expect(fmtDb(0)).toBe('0 dB');
    expect(fmtDb(-3.46)).toBe('-3.5 dB');
    expect(fmtDb(10)).toBe('+10 dB');
    expect(fmtDb(undefined)).toBe('—');
    expect(fmtPan(0)).toBe('C');
    expect(fmtPan(-0.5)).toBe('L50');
    expect(fmtPan(1)).toBe('R100');
  });
});
