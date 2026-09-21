/**
 * Renders a `Document` as one self-contained HTML file: the theme as CSS
 * variables, a table of contents, tables with a live filter box, diagrams
 * as inline SVG, and a print stylesheet that starts each section on a new
 * page — so "print to PDF" from a browser gives the same book as the PDF
 * export.
 */
import { cell, type Block, type CellLike, type Diagram, type Document } from './ir';
import { blend, onColor, resolveColor, type Theme } from './theme';
import { pathData } from './diagrams';

const esc = (s: unknown) =>
  String(s ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');

const FONTS: Record<Theme['font'], string> = {
  sans: "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif",
  serif: "Georgia, 'Times New Roman', Times, serif",
  mono: "ui-monospace, Menlo, Consolas, monospace",
};

function css(t: Theme): string {
  return `
:root{--page:${t.page};--text:${t.text};--muted:${t.muted};--heading:${t.heading};--accent:${t.accent};--rule:${t.rule};--thead:${t.tableHead};--thead-text:${t.tableHeadText};--stripe:${t.stripe};--tile:${t.tile};--canvas:${t.canvas};--canvas-line:${t.canvasLine};--canvas-text:${t.canvasText};--good:${t.good};--warn:${t.warn};--bad:${t.bad};--scale:${t.scale}}
*{box-sizing:border-box}
html{background:var(--page);color:var(--text);font:calc(14px * var(--scale))/1.5 ${FONTS[t.font]};-webkit-print-color-adjust:exact;print-color-adjust:exact}
body{margin:0}
a{color:var(--accent)}
.wrap{display:grid;grid-template-columns:220px minmax(0,1fr);gap:32px;max-width:1180px;margin:0 auto;padding:24px 16px}
nav{position:sticky;top:16px;align-self:start;font-size:.9em;max-height:calc(100vh - 32px);overflow:auto}
nav ol{list-style:none;margin:8px 0 0;padding:0}
nav li{margin:0 0 4px}
nav a{text-decoration:none;color:var(--text)}
nav a:hover{color:var(--accent)}
nav .brand{font-weight:700;color:var(--heading)}
nav input{width:100%;margin:10px 0 4px;padding:6px 8px;border:1px solid var(--rule);border-radius:6px;background:var(--page);color:var(--text);font:inherit}
nav .count{color:var(--muted);font-size:.85em;min-height:1.2em}
main{min-width:0}
h1,h2,h3{color:var(--heading);line-height:1.2;margin:1.6em 0 .5em}
h1{font-size:1.7em;padding-left:12px;border-left:4px solid var(--accent)}
h2{font-size:1.25em}
h3{font-size:.8em;text-transform:uppercase;letter-spacing:.06em;color:var(--muted)}
p{margin:.4em 0 .8em}
.muted{color:var(--muted)}
.cover{padding:32px 0 8px;border-bottom:1px solid var(--rule);margin-bottom:24px}
.cover h1{font-size:2.4em;border:0;padding:0;margin:0 0 .2em}
.cover .sub{font-size:1.15em;color:var(--muted);margin:0 0 18px}
.prod{display:grid;grid-template-columns:max-content 1fr;gap:4px 18px;padding:12px 16px;margin:0 0 18px;background:var(--tile);border-left:4px solid var(--accent);border-radius:0 6px 6px 0}
.prod dt{color:var(--muted);font-weight:600}
.prod dd{margin:0}
.tiles{display:grid;grid-template-columns:repeat(auto-fill,minmax(150px,1fr));gap:8px;margin:8px 0 16px}
.tile{background:var(--tile);border-radius:6px;padding:10px 12px}
.tile .v{font-size:1.5em;font-weight:700;color:var(--heading);line-height:1.1}
.tile .l{font-size:.8em;color:var(--muted);margin-top:4px}
.tile .h{font-size:.75em;color:var(--muted);float:right}
.kv{display:grid;grid-template-columns:max-content 1fr;gap:2px 16px;margin:6px 0 12px}
.kv dt{color:var(--muted);font-weight:600}
.kv dd{margin:0}
table{border-collapse:collapse;width:100%;margin:6px 0 18px;font-size:.92em}
table.small{font-size:.8em}
table.tiny{font-size:.72em}
th,td{text-align:left;vertical-align:top;padding:4px 6px;border-bottom:1px solid var(--rule)}
th{background:var(--thead);color:var(--thead-text);font-weight:700;position:sticky;top:0;border-bottom:2px solid var(--rule)}
tbody tr:nth-child(even) td{background:var(--stripe)}
td.r,th.r{text-align:right}
td.c,th.c{text-align:center}
td.b{font-weight:700}
td.m{color:var(--muted)}
.sw{display:inline-block;width:.75em;height:.75em;border-radius:2px;vertical-align:-1px;margin-right:5px}
.dot{display:inline-block;width:.55em;height:.55em;border-radius:50%;background:var(--accent);vertical-align:0}
.dot.t{margin-right:5px}
td.hit{color:inherit}
figure{margin:8px 0 18px}
figure svg{width:100%;height:auto;display:block}
figcaption{font-size:.8em;color:var(--muted);margin-top:4px}
.gloss dt{font-weight:700;margin-top:10px}
.gloss dd{margin:2px 0 0 14px}
footer{margin-top:40px;padding-top:12px;border-top:1px solid var(--rule);color:var(--muted);font-size:.85em}
.ink table th{background:transparent;border-bottom:2px solid var(--text)}
.ink .tile,.ink .prod{background:transparent;border:1px solid var(--rule)}
.ink tbody tr:nth-child(even) td{background:transparent}
tr.hide{display:none}
@media (max-width:900px){.wrap{grid-template-columns:1fr;gap:12px}nav{position:static;max-height:none}nav ol{display:flex;flex-wrap:wrap;gap:4px 12px}}
@media print{.wrap{display:block;max-width:none;padding:0}nav{display:none}section.s{break-before:page}th{position:static}h1{break-after:avoid}tr,figure,.tile{break-inside:avoid}html{font-size:11px}@page{margin:14mm}}
`;
}

function svg(d: Diagram, theme: Theme): string {
  const c = (ref: string | undefined, fallback = theme.text) => resolveColor(theme, ref, fallback);
  const parts: string[] = [];
  if (d.canvas) parts.push(`<rect x="0" y="0" width="${d.w}" height="${d.h}" fill="${theme.canvas}" stroke="${theme.canvasLine}" stroke-width="0.6"/>`);
  for (const it of d.items) {
    const op = it.opacity !== undefined ? ` opacity="${it.opacity}"` : '';
    switch (it.t) {
      case 'rect':
        parts.push(
          `<rect x="${it.x}" y="${it.y}" width="${it.w}" height="${it.h}"${it.r ? ` rx="${it.r}"` : ''} fill="${it.fill ? c(it.fill) : 'none'}"${it.fill && it.opacity !== undefined ? ` fill-opacity="${it.opacity}"` : ''}${it.stroke ? ` stroke="${c(it.stroke)}" stroke-width="${it.sw ?? 0.6}"${it.strokeOpacity !== undefined ? ` stroke-opacity="${it.strokeOpacity}"` : ''}${it.dash ? ` stroke-dasharray="${it.dash.join(' ')}"` : ''}` : ''}/>`,
        );
        break;
      case 'text':
        parts.push(
          `<text x="${it.x}" y="${it.y}" font-size="${it.size}" fill="${c(it.color)}"${it.bold ? ' font-weight="700"' : ''}${it.anchor ? ` text-anchor="${it.anchor}"` : ''}${it.mono ? ' font-family="ui-monospace, Menlo, monospace"' : ''}${op}>${esc(it.text)}</text>`,
        );
        break;
      case 'line':
        parts.push(`<line x1="${it.x1}" y1="${it.y1}" x2="${it.x2}" y2="${it.y2}" stroke="${c(it.stroke, theme.rule)}" stroke-width="${it.sw ?? 0.6}"${it.dash ? ` stroke-dasharray="${it.dash.join(' ')}"` : ''}${op}/>`);
        break;
      case 'path':
        parts.push(`<path d="${pathData(it.points, it.curve, it.close)}" fill="${it.fill ? c(it.fill) : 'none'}"${it.stroke ? ` stroke="${c(it.stroke)}" stroke-width="${it.sw ?? 0.8}"` : ''} stroke-linejoin="round" stroke-linecap="round"${op}/>`);
        break;
      case 'circle':
        parts.push(`<circle cx="${it.cx}" cy="${it.cy}" r="${it.r}" fill="${it.fill ? c(it.fill) : 'none'}"${it.stroke ? ` stroke="${c(it.stroke)}" stroke-width="${it.sw ?? 0.6}"` : ''}${op}/>`);
        break;
    }
  }
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${d.w} ${d.h}" font-family="${esc(FONTS[theme.font])}">${parts.join('')}</svg>`;
}

function td(cl: CellLike, theme: Theme, align?: string, header = false): string {
  const c = cell(cl);
  const classes: string[] = [];
  const a = c.align ?? align;
  if (a === 'r' || a === 'c') classes.push(a);
  if (c.bold) classes.push('b');
  if (c.muted) classes.push('m');
  let style = '';
  if (!header && (c.fill || c.heat !== undefined) && !theme.ink) {
    const bg = c.fill ? resolveColor(theme, c.fill) : blend(theme.page, theme.accent, 0.1 + 0.75 * Math.max(0, Math.min(1, c.heat ?? 0)));
    style = `background:${bg}${(c.heat ?? 0) > 0.55 || c.fill ? `;color:${onColor(bg)}` : ''}`;
    classes.push('hit');
  } else if (!header && theme.ink && (c.heat ?? 0) > 0) style = `box-shadow:inset 0 -${(0.5 + 1.5 * (c.heat ?? 0)).toFixed(1)}px 0 var(--text)`;
  if (c.color && !style) style = `color:${resolveColor(theme, c.color)}`;
  const inner: string[] = [];
  if (c.swatch) inner.push(`<span class="sw" style="background:${resolveColor(theme, c.swatch)}"></span>`);
  if (c.dot) inner.push(`<span class="dot${c.text ? ' t' : ''}"${c.dot !== true ? ` style="background:${resolveColor(theme, c.dot)}"` : ''}></span>`);
  inner.push(esc(c.text));
  const tag = header ? 'th' : 'td';
  return `<${tag}${classes.length ? ` class="${classes.join(' ')}"` : ''}${style ? ` style="${style}"` : ''}>${inner.join('')}</${tag}>`;
}

function table(b: Extract<Block, { kind: 'table' }>, theme: Theme): string {
  const head = b.headers.map((h, i) => td({ text: h, swatch: b.headerSwatches?.[i] }, theme, b.align?.[i], true)).join('');
  const rows = b.rows.map((r) => `<tr>${r.map((c, i) => td(c, theme, b.align?.[i])).join('')}</tr>`).join('');
  return `<table class="${b.size ?? 'normal'}"><thead><tr>${head}</tr></thead><tbody>${rows}</tbody></table>`;
}

const FILTER_SCRIPT = `
(function(){var q=document.getElementById('q'),n=document.getElementById('qn');if(!q)return;var rows=[].slice.call(document.querySelectorAll('tbody tr'));
q.addEventListener('input',function(){var s=q.value.trim().toLowerCase(),hit=0;rows.forEach(function(r){var ok=!s||r.textContent.toLowerCase().indexOf(s)>=0;r.classList.toggle('hide',!ok);if(ok&&s)hit++;});n.textContent=s?hit+' matching row'+(hit===1?'':'s'):'';});})();`;

export function renderHtml(doc: Document, theme: Theme): string {
  const toc: { id: string; text: string }[] = [];
  const out: string[] = [];
  let open = false;
  const endSection = () => {
    if (open) out.push('</section>');
    open = false;
  };
  for (const b of doc.blocks) {
    switch (b.kind) {
      case 'cover':
        out.push(`<header class="cover"><h1>${esc(b.title)}</h1><p class="sub">${esc(b.subtitle)}</p>`);
        if (b.production.length) out.push(`<dl class="prod">${b.production.map(([k, v]) => `<dt>${esc(k)}</dt><dd>${esc(v)}</dd>`).join('')}</dl>`);
        out.push(`<div class="tiles">${b.tiles.map((t) => `<div class="tile">${t.hint ? `<span class="h">${esc(t.hint)}</span>` : ''}<div class="v">${esc(t.value)}</div><div class="l">${esc(t.label)}</div></div>`).join('')}</div>`);
        for (const l of b.lines) out.push(`<p class="muted">${esc(l)}</p>`);
        if (b.notes) out.push(`<h2>Show notes</h2><p>${esc(b.notes).replace(/\n/g, '<br>')}</p>`);
        out.push('</header>');
        break;
      case 'h1':
        endSection();
        toc.push({ id: b.id, text: b.text });
        out.push(`<section class="s" id="${esc(b.id)}"><h1>${esc(b.text)}</h1>`);
        open = true;
        break;
      case 'h2':
        out.push(`<h2>${esc(b.text)}</h2>`);
        break;
      case 'h3':
        out.push(`<h3>${esc(b.text)}</h3>`);
        break;
      case 'p':
        out.push(`<p${b.muted ? ' class="muted"' : ''}>${esc(b.text).replace(/\n/g, '<br>')}</p>`);
        break;
      case 'kv':
        out.push(`<dl class="kv">${b.rows.map(([k, v]) => `<dt>${esc(k)}</dt><dd>${esc(v)}</dd>`).join('')}</dl>`);
        break;
      case 'tiles':
        out.push(`<div class="tiles">${b.items.map((t) => `<div class="tile">${t.hint ? `<span class="h">${esc(t.hint)}</span>` : ''}<div class="v">${esc(t.value)}</div><div class="l">${esc(t.label)}</div></div>`).join('')}</div>`);
        break;
      case 'table':
        out.push(table(b, theme));
        break;
      case 'diagram':
        out.push(`<figure>${svg(b.d, theme)}${b.caption ? `<figcaption>${esc(b.caption)}</figcaption>` : ''}</figure>`);
        break;
      case 'glossary':
        out.push(`<dl class="gloss">${b.entries.map((g) => `<dt>${esc(g.term)}</dt><dd>${esc(g.text)}</dd>`).join('')}</dl>`);
        break;
      case 'gap':
        out.push(`<div style="height:${b.h}px"></div>`);
        break;
      case 'pagebreak':
        break;
    }
  }
  endSection();
  const nav = `<nav><div class="brand">${esc(doc.title)}</div><input id="q" type="search" placeholder="Filter table rows…" aria-label="Filter table rows"><div class="count" id="qn"></div><ol>${toc.map((t) => `<li><a href="#${esc(t.id)}">${esc(t.text)}</a></li>`).join('')}</ol></nav>`;
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="generator" content="${esc(doc.producer)}">
<title>${esc(doc.title)}</title>
<style>${css(theme)}</style>
</head>
<body class="${theme.ink ? 'ink ' : ''}theme-${theme.id}">
<div class="wrap">${nav}<main>${out.join('\n')}<footer>${esc(doc.subtitle)} · generated by ${esc(doc.producer)}</footer></main></div>
<script>${FILTER_SCRIPT}</script>
</body>
</html>
`;
}
