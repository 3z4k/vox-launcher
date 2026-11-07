// Connects the UI to the Rust engine through Tauri's global API and exposes it as `api`.
import type {
  AccountInfo, Conflict, ContentKind, InstanceUpdates, PackUpdate, Snapshot, Events, FileEntry, ImportCandidate, InstallPlan, InstanceConfig, JavaInstall, Loader, LogFile, McVersion, ModVersion, NewInstance,
  ClientStatus, CrashReport, LauncherUpdate, LibSkin, Screenshot, SearchResult, Server, Settings, SkinInfo, UpdateInfo, World,
} from './types';

interface TauriGlobal {
  core: { invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T>; convertFileSrc(path: string): string };
  event: { listen<T>(name: string, cb: (e: { payload: T }) => void): Promise<() => void> };
}
const tauri = (window as unknown as { __TAURI__: TauriGlobal }).__TAURI__;
const call = <T>(cmd: string, args?: Record<string, unknown>): Promise<T> => tauri.core.invoke<T>(cmd, args);

const CHANNELS: (keyof Events)[] = ['game:progress', 'game:log', 'game:state', 'auth:code', 'task:progress', 'task:speed', 'app:launch'];

export const api = {
  listVersions: () => call<McVersion[]>('list_versions'),
  listLoaderVersions: (loader: Loader, mcVersion: string) => call<string[]>('list_loader_versions', { loader, mcVersion }),

  listInstances: () => call<InstanceConfig[]>('list_instances'),
  createInstance: (input: NewInstance) => call<InstanceConfig>('create_instance', { input }),
  updateInstance: (id: string, patch: Partial<InstanceConfig>) => call<InstanceConfig>('update_instance', { id, patch }),
  deleteInstance: (id: string) => call<void>('delete_instance', { id }),
  duplicateInstance: (id: string) => call<InstanceConfig>('duplicate_instance', { id }),
  openPath: (id: string, sub = '') => call<void>('open_path', { id, sub }),
  isPortable: () => call<boolean>('is_portable'),
  dataDir: () => call<string>('data_dir'),
  openDataDir: () => call<void>('open_data_dir'),
  pickFile: (kind: 'import' | 'skin' | 'content' | 'java' | 'any') => call<string | null>('pick_file', { kind }),
  pickFolder: () => call<string | null>('pick_folder'),
  detectImports: () => call<ImportCandidate[]>('detect_imports'),
  scanImportFolder: (path: string) => call<ImportCandidate[]>('scan_import_folder', { path }),
  importInstances: (keys: string[], includeWorlds: boolean, taskId: string) => call<InstanceConfig[]>('import_instances', { keys, includeWorlds, taskId }),
  pickSave: (defaultName: string) => call<string | null>('pick_save', { defaultName }),
  importFile: (path: string, taskId: string) => call<InstanceConfig>('import_file', { path, taskId }),
  exportInstance: (id: string, path: string, includeWorlds: boolean, taskId: string) => call<void>('export_instance', { id, path, includeWorlds, taskId }),

  listAccounts: () => call<AccountInfo[]>('list_accounts'),
  login: () => call<AccountInfo>('login'),
  cancelLogin: () => call<void>('cancel_login'),
  addOfflineAccount: (username: string) => call<AccountInfo>('add_offline_account', { username }),
  removeAccount: (id: string) => call<void>('remove_account', { id }),
  setActiveAccount: (id: string) => call<void>('set_active_account', { id }),
  getSkinInfo: () => call<SkinInfo>('get_skin_info'),
  uploadSkin: (path: string, variant: 'classic' | 'slim') => call<void>('upload_skin', { path, variant }),
  skinLibrary: () => call<LibSkin[]>('skin_library'),
  skinLibraryAdd: (path: string | null, name: string, variant: 'classic' | 'slim') => call<LibSkin>('skin_library_add', { path, name, variant }),
  skinLibraryApply: (id: string, variant: 'classic' | 'slim' | null) => call<void>('skin_library_apply', { id, variant }),
  skinLibraryRemove: (id: string) => call<void>('skin_library_remove', { id }),
  skinLibraryUpdate: (id: string, name: string, variant: 'classic' | 'slim') => call<void>('skin_library_update', { id, name, variant }),
  resetSkin: () => call<void>('reset_skin'),
  setCape: (capeId: string | null) => call<void>('set_cape', { capeId }),

  getSettings: () => call<Settings>('get_settings'),
  saveSettings: (settings: Settings) => call<Settings>('save_settings', { settings }),
  detectJavas: () => call<JavaInstall[]>('detect_javas'),
  testJava: (path: string) => call<number>('test_java', { path }),

  listFiles: (id: string, kind: ContentKind) => call<FileEntry[]>('list_files', { id, kind }),
  setFileEnabled: (id: string, kind: ContentKind, name: string, enabled: boolean) => call<void>('set_file_enabled', { id, kind, name, enabled }),
  deleteFile: (id: string, kind: ContentKind, name: string) => call<void>('delete_file', { id, kind, name }),
  addFile: (id: string, kind: ContentKind, path: string) => call<string>('add_file', { id, kind, path }),
  checkUpdates: (id: string, kind: ContentKind) => call<UpdateInfo[]>('check_updates', { id, kind }),
  applyUpdate: (id: string, kind: ContentKind, filename: string, versionId: string) => call<string>('apply_update', { id, kind, filename, versionId }),
  listWorlds: (id: string) => call<World[]>('list_worlds', { id }),
  deleteWorld: (id: string, folder: string) => call<void>('delete_world', { id, folder }),
  backupWorld: (id: string, folder: string) => call<string>('backup_world', { id, folder }),
  listScreenshots: (id: string) => call<Screenshot[]>('list_screenshots', { id }),
  deleteScreenshot: (id: string, name: string) => call<void>('delete_screenshot', { id, name }),
  listServers: (id: string) => call<Server[]>('list_servers', { id }),
  saveServers: (id: string, servers: Server[]) => call<void>('save_servers', { id, servers }),
  listLogs: (id: string) => call<LogFile[]>('list_logs', { id }),
  readLog: (id: string, path: string) => call<string>('read_log', { id, path }),
  syncOptions: (id: string, source: string, files: string[]) => call<string[]>('sync_options', { id, source, files }),
  hasVanillaDir: () => call<boolean>('has_vanilla_dir'),

  mrSearchFiltered: (filters: { query: string; kind: ContentKind | 'modpacks'; mc: string; loaders: string[]; categories: string[]; environment: string; sort: string; offset: number; limit: number }) => call<SearchResult>('mr_search_filtered', { filters }),
  mrSearch: (query: string, kind: ContentKind | 'modpacks', mc: string, loader: Loader | null, index: string, offset: number) =>
    call<SearchResult>('mr_search', { query, kind, mc, loader, index, offset }),
  mrVersions: (projectId: string, kind: ContentKind | 'modpacks', mc: string, loader: Loader | null) =>
    call<ModVersion[]>('mr_versions', { projectId, kind, mc, loader }),
  mrInstall: (id: string, kind: ContentKind, versionId: string, title: string, iconUrl: string | null, taskId: string) =>
    call<string[]>('mr_install', { id, kind, versionId, title, iconUrl, taskId }),
  mrPlan: (id: string, kind: ContentKind, versionId: string, title: string) => call<InstallPlan>('mr_plan', { id, kind, versionId, title }),
  mrInstallModpack: (versionId: string, taskId: string) => call<InstanceConfig>('mr_install_modpack', { versionId, taskId }),

  analyzeCrash: (id: string, extra: string, exitCode: number | null) => call<CrashReport>('analyze_crash', { id, extra, exitCode }),
  appVersion: () => call<string>('app_version'),
  checkLauncherUpdate: () => call<LauncherUpdate | null>('check_launcher_update'),
  installLauncherUpdate: (url: string, name: string, size: number | null, taskId: string) => call<void>('install_launcher_update', { url, name, size, taskId }),
  uninstallApp: (deleteData: boolean) => call<void>('uninstall_app', { deleteData }),
  notify: (title: string, body: string, force = false) => call<void>('notify', { title, body, force }),
  clientStatus: (id: string) => call<ClientStatus>('client_status', { id }),
  clientInstall: (id: string, taskId: string) => call<string>('client_install', { id, taskId }),
  clientRemove: (id: string) => call<void>('client_remove', { id }),

  accountSkins: () => call<Record<string, string>>('account_skins'),
  listSnapshots: (id: string) => call<Snapshot[]>('list_snapshots', { id }),
  createSnapshot: (id: string, name: string) => call<Snapshot>('create_snapshot', { id, name }),
  restoreSnapshot: (id: string, snapshot: string) => call<InstanceConfig>('restore_snapshot', { id, snapshot }),
  deleteSnapshot: (id: string, snapshot: string) => call<void>('delete_snapshot', { id, snapshot }),
  checkConflicts: (id: string) => call<Conflict[]>('check_conflicts', { id }),
  packUpdateCheck: (id: string) => call<PackUpdate | null>('pack_update_check', { id }),
  packUpdate: (id: string, versionId: string, taskId: string) => call<InstanceConfig>('pack_update', { id, versionId, taskId }),
  updateAllMods: (taskId: string) => call<InstanceUpdates[]>('update_all_mods', { taskId }),
  exportTheme: (json: string, defaultName: string) => call<boolean>('export_theme', { json, defaultName }),
  importTheme: () => call<string | null>('import_theme'),
  taskPause: (id: string, paused: boolean) => call<void>('task_pause', { id, paused }),
  taskCancel: (id: string) => call<void>('task_cancel', { id }),
  takeLaunchRequest: () => call<string | null>('take_launch_request'),
  createShortcut: (id: string, place: 'desktop' | 'startmenu') => call<string>('create_shortcut', { id, place }),
  launch: (id: string, join?: { server?: string; world?: string }) => call<void>('launch', { id, joinServer: join?.server ?? null, joinWorld: join?.world ?? null }),
  kill: (id: string) => call<void>('kill', { id }),
  running: () => call<string[]>('running'),
  openExternal: (url: string) => call<void>('open_external', { url }),

  /** Local file path -> URL the webview can load (screenshots, world icons). */
  fileUrl: (path: string) => tauri.core.convertFileSrc(path),

  on<K extends keyof Events>(channel: K, cb: (payload: Events[K]) => void): () => void {
    if (!CHANNELS.includes(channel)) throw new Error(`Unknown channel ${channel}`);
    let off: (() => void) | null = null;
    let cancelled = false;
    void tauri.event.listen<Events[K]>(channel, (e) => cb(e.payload)).then((fn) => {
      if (cancelled) fn();
      else off = fn;
    });
    return () => {
      cancelled = true;
      off?.();
    };
  },
};

export type Api = typeof api;
