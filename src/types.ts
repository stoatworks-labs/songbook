/**
 * The JSON shapes shared with Rust — a mirror of `songbook-model`. Read this
 * first: every view draws from these and nothing in the UI computes a value
 * Rust has not already put here.
 */

export type Platform =
  | 'ah-sq'
  | 'ah-dlive'
  | 'ah-avantis'
  | 'ah-qu'
  | 'ah-cq'
  | 'yamaha-cl-ql'
  | 'yamaha-tf'
  | 'yamaha-dm3'
  | 'yamaha-dm7'
  | 'yamaha-rivage'
  | 'generic';

export const PLATFORM_LABEL: Record<Platform, string> = {
  'ah-sq': 'Allen & Heath SQ',
  'ah-dlive': 'Allen & Heath dLive',
  'ah-avantis': 'Allen & Heath Avantis',
  'ah-qu': 'Allen & Heath Qu',
  'ah-cq': 'Allen & Heath CQ',
  'yamaha-cl-ql': 'Yamaha CL / QL',
  'yamaha-tf': 'Yamaha TF',
  'yamaha-dm3': 'Yamaha DM3',
  'yamaha-dm7': 'Yamaha DM7',
  'yamaha-rivage': 'Yamaha RIVAGE PM',
  generic: 'Generic',
};

export const isAh = (p: Platform): boolean => p.startsWith('ah-');
export const isYamaha = (p: Platform): boolean => p.startsWith('yamaha-');

/** Fader / send level that stands for −∞ (JSON has no infinity). */
export const FADER_OFF = -144;

export const COLORS = ['off', 'red', 'green', 'yellow', 'blue', 'purple', 'cyan', 'white', 'orange', 'pink'] as const;
export type Color = (typeof COLORS)[number];

export const COLOR_HEX: Record<string, string> = {
  off: '#555a66',
  red: '#e0443e',
  green: '#3bb45c',
  yellow: '#e6c034',
  blue: '#3b7be0',
  purple: '#8f5bd6',
  cyan: '#3bc0d6',
  white: '#e8e8e8',
  orange: '#e88a2e',
  pink: '#e05a9e',
};

export type Extra = Record<string, unknown>;

export type UnitRole = 'console' | 'surface' | 'mix-rack' | 'stage-box' | 'card' | 'network' | 'internal';

export interface Unit {
  id: string;
  label: string;
  model: string;
  role: UnitRole;
  address?: string;
  extra?: Extra;
}

export type SocketKind = 'mic' | 'line' | 'aes' | 'dante' | 's-link' | 'usb' | 'madi' | 'card' | 'internal' | 'other';

export interface Socket {
  id: string;
  unitId: string;
  kind: SocketKind;
  direction: 'in' | 'out';
  index: number;
  label: string;
  extra?: Extra;
}

export interface Preamp {
  socketId: string;
  gainDb?: number;
  pad?: boolean;
  phantom?: boolean;
  extra?: Extra;
}

export interface System {
  model: string;
  firmware: string;
  name: string;
  sampleRate?: number;
  clockSource?: string;
  units: Unit[];
  extra?: Extra;
}

export type ChannelKind = 'input' | 'stereo-input' | 'fx-return';

export interface Filter {
  on: boolean;
  freqHz?: number;
}

export type BandKind = 'low-shelf' | 'bell' | 'high-shelf' | 'notch' | 'low-pass' | 'high-pass' | 'other';

export interface EqBand {
  kind: BandKind;
  freqHz: number;
  gainDb: number;
  q?: number;
  on: boolean;
}

export interface Eq {
  on: boolean;
  bands: EqBand[];
}

export interface Dynamics {
  on: boolean;
  thresholdDb?: number;
  ratio?: number;
  attackMs?: number;
  releaseMs?: number;
  holdMs?: number;
  rangeDb?: number;
  knee?: string;
}

export interface Strip {
  faderDb?: number;
  on?: boolean;
  pan?: number;
  mainAssign?: boolean;
  polarity?: boolean;
  trimDb?: number;
  hpf?: Filter;
  lpf?: Filter;
  eq?: Eq;
  gate?: Dynamics;
  comp?: Dynamics;
  delayMs?: number;
  insertOn?: boolean;
}

export interface Send {
  busId: string;
  levelDb?: number;
  on?: boolean;
  pre?: boolean;
  pan?: number;
}

export interface Channel {
  id: string;
  number: number;
  kind: ChannelKind;
  label: string;
  color?: string;
  stereo: boolean;
  source?: string;
  sourceRight?: string;
  strip: Strip;
  sends: Send[];
  dcaIds: string[];
  muteGroupIds: string[];
  extra?: Extra;
}

export type BusKind = 'main' | 'aux' | 'group' | 'matrix' | 'fx-send' | 'other';

export const BUS_KIND_LABEL: Record<BusKind, string> = {
  main: 'Main',
  aux: 'Aux',
  group: 'Group',
  matrix: 'Matrix',
  'fx-send': 'FX send',
  other: 'Other',
};

export interface Bus {
  id: string;
  number: number;
  kind: BusKind;
  label: string;
  color?: string;
  stereo: boolean;
  strip: Strip;
  sends: Send[];
  dcaIds: string[];
  muteGroupIds: string[];
  extra?: Extra;
}

export interface Dca {
  id: string;
  number: number;
  label: string;
  color?: string;
  faderDb?: number;
  on?: boolean;
  extra?: Extra;
}

export interface MuteGroup {
  id: string;
  number: number;
  label: string;
  on?: boolean;
  extra?: Extra;
}

export type OutputSourceKind = 'bus' | 'channel' | 'direct-out' | 'socket' | 'other';

export interface OutputSource {
  kind: OutputSourceKind;
  refId?: string;
  label: string;
}

export interface OutputPatch {
  socketId: string;
  source: OutputSource;
  extra?: Extra;
}

export interface Snapshot {
  channels: Channel[];
  buses: Bus[];
  dcas: Dca[];
  muteGroups: MuteGroup[];
  preamps: Preamp[];
}

export interface Scene {
  id: string;
  number?: number;
  bank?: string;
  label: string;
  notes: string;
  filters: string[];
  snapshot?: Snapshot;
  extra?: Extra;
}

export type CueStepKind = 'recall-scene' | 'wait' | 'midi' | 'other';

export interface CueStep {
  kind: CueStepKind;
  sceneId?: string;
  delayMs?: number;
  extra?: Extra;
}

export interface Cue {
  id: string;
  number?: number;
  label: string;
  notes: string;
  steps: CueStep[];
  extra?: Extra;
}

export interface VendorBlob {
  platform: Platform;
  kind: string;
  sha256: string;
  file: string;
  size: number;
  capturedAt: string;
  note: string;
}

export type NoteLevel = 'info' | 'adapted' | 'dropped';

export interface Note {
  level: NoteLevel;
  path: string;
  message: string;
}

export interface SourceInfo {
  kind: string;
  origin: string;
  at: string;
  firmware?: string;
}

/** Who the show is for and who is mixing it — printed on the documentation. */
export interface Production {
  /** The show or event name, if it differs from the file name. */
  event?: string;
  client?: string;
  company?: string;
  venue?: string;
  /** Show date(s), free text: "12–14 March 2026", "Fri 3 Oct". */
  date?: string;
  /** Engineer on the desk. */
  operator?: string;
  /** Phone or email for whoever is on the desk. */
  contact?: string;
}

export interface Meta {
  name: string;
  notes: string;
  tags: string[];
  created: string;
  modified: string;
  author?: string;
  source?: SourceInfo;
  production?: Production;
}

export interface Show {
  schema: string;
  id: string;
  meta: Meta;
  platform: Platform;
  system: System;
  sockets: Socket[];
  preamps: Preamp[];
  channels: Channel[];
  buses: Bus[];
  dcas: Dca[];
  muteGroups: MuteGroup[];
  outputPatch: OutputPatch[];
  scenes: Scene[];
  cues: Cue[];
  vendor: VendorBlob[];
  notes: Note[];
}

// ---- library ----

export interface Summary {
  id: string;
  name: string;
  platform: Platform;
  model: string;
  firmware: string;
  modified: string;
  inputs: number;
  outputs: number;
  channels: number;
  namedChannels: number;
  auxes: number;
  groups: number;
  matrices: number;
  fx: number;
  dcas: number;
  muteGroups: number;
  scenes: number;
  cues: number;
  notesDropped: number;
}

export interface Entry {
  summary: Summary;
  dir: string;
  commits: number;
  vendorFiles: number;
}

export interface Commit {
  id: string;
  parent?: string;
  at: string;
  message: string;
  author?: string;
  hash: string;
  summary: Summary;
  changes: number;
}

export interface Change {
  kind: 'added' | 'removed' | 'changed';
  path: string;
  before?: unknown;
  after?: unknown;
}

// ---- settings / devices ----

export interface DeviceEntry {
  name: string;
  platform: Platform;
  host: string;
  midiChannel?: number;
  model?: string;
}

export interface Tokens {
  accessToken: string;
  refreshToken?: string;
  expiresAt?: number;
  scope: string;
  account: string;
}

export interface SyncSettings {
  provider: string;
  root: string;
  clientId: string;
  clientSecret?: string;
  tokens?: Tokens;
  lastRun: string;
}

export interface Settings {
  libraryPath: string;
  author: string;
  devices: DeviceEntry[];
  sync: SyncSettings;
  companionHost: string;
}

export interface AppInfo {
  version: string;
  configDir: string;
  platforms: { id: Platform; label: string; vendor: string; models: string[] }[];
}

export interface Capabilities {
  platform: Platform;
  model: string;
  inputChannels: number;
  stereoInputs: number;
  fxReturns: number;
  localInputs: number;
  localOutputs: number;
  auxes: number;
  groups: number;
  mixPool: number;
  matrices: number;
  fxSends: number;
  mains: number;
  dcas: number;
  muteGroups: number;
  sceneSlots: number;
  nameLength: number;
  colors: string[];
  eqBands: number;
  preampControl: boolean;
  cues: boolean;
  notes: string[];
}

export interface SyncReport {
  uploaded: string[];
  downloaded: string[];
  unchanged: number;
  errors: string[];
}

export interface ImportedButton {
  page: number;
  row: number;
  column: number;
  text: string;
  module: string;
  definition: string;
  options: Record<string, unknown>;
  matched?: string;
  problem?: string;
}

export interface CompanionImportReport {
  fileVersion: number;
  kind: string;
  pages: string[];
  connections: string[];
  buttons: ImportedButton[];
  unmatched: number;
}

export interface CompanionExportOptions {
  connectionLabel: string;
  host: string;
  includeChannels: boolean;
  includeDcas: boolean;
  includeMuteGroups: boolean;
  includeScenes: boolean;
  pageName: string;
}

export const tail = (id: string): string => (id.includes(':') ? id.slice(id.lastIndexOf(':') + 1) : id);

/** `−∞`, `+3.5 dB`, `0 dB`, or `—` for unknown. */
export function fmtDb(v?: number): string {
  if (v === undefined || v === null) return '—';
  if (v <= FADER_OFF + 0.5) return '−∞';
  const r = Math.round(v * 10) / 10;
  const s = Number.isInteger(r) ? String(r) : r.toFixed(1);
  return `${r > 0 ? '+' : ''}${s} dB`;
}

/** `L50`, `C`, `R20`. */
export function fmtPan(p?: number): string {
  if (p === undefined || p === null) return '—';
  const pct = Math.round(Math.abs(p) * 100);
  if (pct === 0) return 'C';
  return `${p < 0 ? 'L' : 'R'}${pct}`;
}

export function socketLabel(show: Show, id?: string): string {
  if (!id) return '';
  const s = show.sockets.find((x) => x.id === id);
  if (!s) return id;
  const u = show.system.units.find((x) => x.id === s.unitId);
  // The desk's own sockets are named plainly; a stage box or a stream says which.
  if (!u || u.role === 'console' || u.role === 'surface') return s.label;
  const unit = u.label;
  return s.label.toLowerCase().includes(unit.toLowerCase().split(' ')[0]) ? s.label : `${unit} · ${s.label}`;
}

export function busLabel(show: Show, id: string): string {
  return show.buses.find((b) => b.id === id)?.label ?? id;
}
