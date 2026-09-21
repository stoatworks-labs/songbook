/**
 * The show documentation, in whichever format is asked for. The content is
 * built once as a `Document` (see `document.ts`) and rendered by pdf-lib or
 * as a self-contained HTML page, so both say the same thing in the same
 * theme.
 */
import { buildDocument, DEFAULT_DOC_OPTIONS, type DocOptions } from './document';
import { renderHtml } from './doc/render-html';
import { renderPdf } from './doc/render-pdf';
import { THEMES, type PaperId, type ThemeId } from './doc/theme';
import type { Commit, Show } from '../types';

export type { DocOptions } from './document';
export { DEFAULT_DOC_OPTIONS } from './document';
/** Kept for callers written against the old name. */
export type PdfOptions = DocOptions;

export type DocFormat = 'pdf' | 'html';

export const DOC_FORMATS: { id: DocFormat; label: string; ext: string; hint: string }[] = [
  { id: 'pdf', label: 'PDF', ext: 'pdf', hint: 'Paged, for printing and sending.' },
  { id: 'html', label: 'Web page', ext: 'html', hint: 'One file: searchable, filterable tables, prints to PDF from a browser.' },
];

const theme = (o: DocOptions) => THEMES[o.theme ?? 'light'] ?? THEMES.light;
const paper = (o: DocOptions): PaperId => o.paper ?? 'a4';

export async function buildPdf(show: Show, commits: Commit[], opts: DocOptions = DEFAULT_DOC_OPTIONS): Promise<Uint8Array> {
  return renderPdf(buildDocument(show, commits, opts), theme(opts), paper(opts));
}

export function buildHtml(show: Show, commits: Commit[], opts: DocOptions = DEFAULT_DOC_OPTIONS): string {
  return renderHtml(buildDocument(show, commits, opts), theme(opts));
}

export interface BuiltDoc {
  format: DocFormat;
  ext: string;
  /** PDF bytes, or the HTML page as text. */
  bytes?: Uint8Array;
  text?: string;
}

export async function buildDoc(show: Show, commits: Commit[], opts: DocOptions, format: DocFormat): Promise<BuiltDoc> {
  if (format === 'html') return { format, ext: 'html', text: buildHtml(show, commits, opts) };
  return { format: 'pdf', ext: 'pdf', bytes: await buildPdf(show, commits, opts) };
}

export type { ThemeId };
