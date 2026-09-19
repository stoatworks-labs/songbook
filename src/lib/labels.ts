/**
 * Printable label strips: one row of channel names, coloured as on the desk,
 * at the fader pitch you give it, rendered on a canvas and saved as a PNG
 * at 300 dpi so it prints to size. The default pitch is not a measured
 * figure for any desk — set it to yours (measure centre to centre across
 * ten faders and divide by ten).
 */
import { COLOR_HEX, type Show } from '../types';

export interface LabelOptions {
  /** Fader pitch in millimetres. */
  pitchMm: number;
  /** Label height in millimetres. */
  heightMm: number;
  /** Faders per strip (a bank / layer of the desk). */
  perStrip: number;
  /** Include the channel number. */
  numbers: boolean;
}

export const DEFAULT_LABELS: LabelOptions = { pitchMm: 25, heightMm: 14, perStrip: 16, numbers: true };

const DPI = 300;
const mm = (v: number) => (v / 25.4) * DPI;

export function safeName(s: string): string {
  return s.replace(/[^\w.-]+/g, '_').replace(/^_+|_+$/g, '') || 'show';
}

/** One strip for channels `from..from+perStrip`. */
export function labelStrip(show: Show, from: number, opts: LabelOptions): HTMLCanvasElement {
  const channels = show.channels.filter((c) => c.kind === 'input').slice(from, from + opts.perStrip);
  const n = Math.max(channels.length, 1);
  const c = document.createElement('canvas');
  c.width = Math.round(mm(opts.pitchMm) * n);
  c.height = Math.round(mm(opts.heightMm));
  const ctx = c.getContext('2d')!;
  ctx.fillStyle = '#ffffff';
  ctx.fillRect(0, 0, c.width, c.height);
  const pw = mm(opts.pitchMm);
  channels.forEach((ch, i) => {
    const x = i * pw;
    const col = COLOR_HEX[ch.color ?? 'off'] ?? COLOR_HEX.off;
    ctx.fillStyle = col;
    ctx.fillRect(x, 0, pw, c.height);
    // Dark text on light colours, light on dark.
    const [r, g, b] = [1, 3, 5].map((k) => parseInt(col.slice(k, k + 2), 16));
    const lum = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
    ctx.fillStyle = lum > 0.6 ? '#101418' : '#ffffff';
    ctx.strokeStyle = 'rgba(0,0,0,0.35)';
    ctx.lineWidth = mm(0.15);
    ctx.strokeRect(x + 0.5, 0.5, pw - 1, c.height - 1);
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    const name = ch.label || `Ch ${ch.number}`;
    let size = mm(opts.heightMm) * 0.42;
    ctx.font = `bold ${size}px Helvetica, Arial, sans-serif`;
    while (ctx.measureText(name).width > pw * 0.9 && size > mm(1.5)) {
      size *= 0.92;
      ctx.font = `bold ${size}px Helvetica, Arial, sans-serif`;
    }
    ctx.fillText(name, x + pw / 2, opts.numbers ? c.height * 0.42 : c.height / 2);
    if (opts.numbers) {
      ctx.font = `${mm(opts.heightMm) * 0.22}px Helvetica, Arial, sans-serif`;
      ctx.fillText(String(ch.number), x + pw / 2, c.height * 0.8);
    }
  });
  return c;
}

export function toPngBase64(canvas: HTMLCanvasElement): Promise<string> {
  return new Promise((resolve, reject) => {
    canvas.toBlob((blob) => {
      if (!blob) return reject(new Error('canvas export failed'));
      const r = new FileReader();
      r.onload = () => resolve(String(r.result).split(',')[1] ?? '');
      r.onerror = () => reject(r.error);
      r.readAsDataURL(blob);
    }, 'image/png');
  });
}
