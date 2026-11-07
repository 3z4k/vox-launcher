// Shapes exchanged with the Rust engine. Field names match src-tauri/src/types.rs (camelCase).

export type Layout = 'cover' | 'dense' | 'glass' | 'classic' | 'compact' | 'right' | 'bar' | 'zen';
export type Theme = 'voxel' | 'sleek' | 'bold' | 'minimal' | 'custom';
export type Loader = 'vanilla' | 'fabric' | 'quilt' | 'forge' | 'neoforge';
export type ContentKind = 'mods' | 'resourcepacks' | 'shaderpacks';

export interface InstanceConfig {
  id: string;
  name: string;
  mcVersion: string;
  loader: Loader;
  loaderVersion: string;
  versionId: string;
  created: number;
  lastPlayed: number | null;
  playedMs: number;
  group: string;
  icon: string;
  notes: string;
  memoryMb: number | null;
  minMemoryMb: number | null;
  javaPath: string;
  jvmArgs: string;
  extraGameArgs: string;
  width: number | null;
  height: number | null;
  fullscreen: boolean | null;
  wrapper: string;
  preLaunch: string;
  postExit: string;
  envVars: string;
  syncOptionsFrom: string;
  joinServer: string;
  joinWorld: string;
  verifyNext?: boolean;
}

export interface NewInstance {
  name: string;
  mcVersion: string;
  loader: Loader;
  loaderVersion: string;
  group: string;
  icon: string;
}

export interface Settings {
  defaultMemoryMb: number;
  defaultMinMemoryMb: number;
  javaPath: string;
  autoDownloadJava: boolean;
  jvmArgs: string;
  defaultWidth: number;
  defaultHeight: number;
  defaultFullscreen: boolean;
  wrapper: string;
  preLaunch: string;
  postExit: string;
  envVars: string;
  launcherAction: 'keep' | 'minimize';
  showConsoleOnLaunch: boolean;
  checkLauncherUpdates: boolean;
  updateRepo: string;
  notifications: boolean;
  consoleMaxLines: number;
  showSnapshots: boolean;
  checkModUpdates: boolean;
  downloadThreads: number;
  proxyUrl: string;
  discordEnabled: boolean;
  discordShowInstance: boolean;
  discordShowTime: boolean;
  discordShowServer: boolean;
  theme: Theme;
  layout: Layout;
  accent: string;
  uiScale: number;
  compact: boolean;
  reduceMotion: boolean;
  warnModConflicts: boolean;
  customTheme: Record<string, string>;
}

export interface AccountInfo {
  id: string;
  name: string;
  active: boolean;
  offline: boolean;
}

export interface McVersion {
  id: string;
  type: 'release' | 'snapshot' | 'old_beta' | 'old_alpha';
  releaseTime: string;
}

export interface DeviceCode {
  userCode: string;
  verificationUri: string;
  expiresIn: number;
}

export interface Cape {
  id: string;
  alias: string;
  active: boolean;
  data: string | null;
}
export interface SkinInfo {
  name: string;
  uuid: string;
  variant: 'classic' | 'slim';
  skin: string | null;
  capes: Cape[];
}

export interface LibSkin {
  id: string;
  name: string;
  variant: 'classic' | 'slim';
  data: string;
}

export interface FileEntry {
  name: string;
  size: number;
  enabled: boolean;
  isDir: boolean;
  modified: number;
  title: string;
  version: string;
  iconUrl: string | null;
  projectId: string | null;
}
export interface World {
  folder: string;
  name: string;
  lastPlayed: number;
  gameType: string;
  size: number;
  iconPath: string | null;
}
export interface Screenshot {
  name: string;
  path: string;
  size: number;
  modified: number;
}
export interface Server {
  name: string;
  ip: string;
  icon: string | null;
}
export interface LogFile {
  path: string;
  size: number;
  modified: number;
}

export interface SearchHit {
  projectId: string;
  slug: string;
  title: string;
  description: string;
  iconUrl: string | null;
  author: string;
  downloads: number;
  follows: number;
  projectType: string;
  categories: string[];
}
export interface SearchResult {
  hits: SearchHit[];
  totalHits: number;
}
export interface VFile {
  url: string;
  filename: string;
  primary: boolean;
  size: number;
  sha1: string;
}
export interface ModVersion {
  id: string;
  projectId: string;
  name: string;
  versionNumber: string;
  versionType: string;
  datePublished: string;
  gameVersions: string[];
  loaders: string[];
  files: VFile[];
  dependencies: { projectId: string | null; versionId: string | null; kind: string }[];
}
export interface InstallPlan {
  items: { projectId: string; versionId: string; title: string; versionNumber: string; role: 'main' | 'dependency' }[];
  alreadyInstalled: string[];
  warnings: string[];
}
export interface UpdateInfo {
  filename: string;
  projectId: string;
  title: string;
  currentVersion: string;
  newVersion: ModVersion;
}
export interface JavaInstall {
  path: string;
  major: number;
}

export interface ProgressEvent {
  id: string;
  stage: string;
  current: number;
  total: number;
}
export interface TaskEvent {
  taskId: string;
  stage: string;
  current: number;
  total: number;
}
export interface SpeedEvent {
  taskId: string;
  bytes: number;
  bps: number;
  paused: boolean;
}
export interface StateEvent {
  id: string;
  running: boolean;
  code?: number | null;
}
export interface Events {
  'game:progress': ProgressEvent;
  'game:log': { id: string; line: string };
  'game:state': StateEvent;
  'auth:code': DeviceCode;
  'task:progress': TaskEvent;
  'task:speed': SpeedEvent;
  'app:launch': string;
}

export interface LauncherUpdate {
  version: string;
  current: string;
  pageUrl: string;
  notes: string;
  assetUrl: string | null;
  assetName: string | null;
  assetSize: number | null;
}

export interface FixAction {
  kind: 'memory' | 'verify' | 'mods' | 'java' | 'account' | 'drivers';
  label: string;
  value: number | null;
}
export interface Finding {
  id: string;
  title: string;
  explanation: string;
  fix: string;
  evidence: string[];
  action: FixAction | null;
}
export interface CrashReport {
  source: string;
  findings: Finding[];
  text: string;
}
export interface ClientStatus {
  installed: string | null;
  supported: string[];
  sourceKnown: boolean;
}

export interface ImportCandidate {
  source: 'prism' | 'multimc' | 'official' | 'curseforge';
  sourceLabel: string;
  key: string;
  name: string;
  mcVersion: string;
  loader: Loader;
  loaderVersion: string;
  mods: number;
  worlds: number;
  problem: string | null;
}

export interface Snapshot {
  id: string;
  name: string;
  created: number;
  mcVersion: string;
  loader: Loader;
  loaderVersion: string;
  mods: string[];
}
export interface Conflict {
  severity: 'error' | 'warn';
  title: string;
  detail: string;
  files: string[];
}
export interface PackUpdate {
  current: string;
  latest: ModVersion;
}
export interface InstanceUpdates {
  id: string;
  name: string;
  updated: { title: string; from: string; to: string }[];
  failed: string[];
  skipped: string | null;
}
