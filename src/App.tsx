import { invoke } from '@tauri-apps/api/core';
import { getVersion } from '@tauri-apps/api/app';
import { LogicalPosition } from '@tauri-apps/api/dpi';
import { startDrag } from '@crabnebula/tauri-plugin-drag';
import { open, save } from '@tauri-apps/plugin-dialog';
import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';
import { relaunch } from '@tauri-apps/plugin-process';
import { check, type DownloadEvent, type Update } from '@tauri-apps/plugin-updater';
import { emitTo, listen } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { getCurrentWebviewWindow, WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { useEffect, useMemo, useRef, useState } from 'react';
import {
  AppWindow, ArrowUpToLine, Check, ChevronDown, ChevronLeft, ChevronRight, ChevronUp, Cloud, Columns3, Copy, File, FileCog, Folder, FolderPlus, Grid2X2, HardDrive,
  ClipboardPaste, Download, FileArchive, FileAudio, FileCode2, FileDown, FileImage, FileJson, FileSpreadsheet, FileText, FileUp, FileVideo, FolderSync, FolderUp, GripVertical, KeyRound, Link2, List, LoaderCircle, LockKeyhole, MoreHorizontal, PanelLeftClose, PanelLeftOpen, Pencil, Plus, RefreshCw, Scissors, Search, Settings, Share2, Trash2, Upload,
} from 'lucide-react';

type Protocol = 'sftp' | 'cloudFtp' | 'ftp' | 'ftps' | 'webdav' | 's3' | 'smb' | 'googleDrive';
type FileEntry = { name: string; path_component?: string; download_name?: string; size: number; modified?: string; permissions?: string; owner?: string; group?: string; file_type: 'File' | 'Directory' | 'Symlink' };
type GoogleDriveLocationKind = 'myDrive' | 'sharedWithMe' | 'sharedDrive';
type GoogleDriveLocation = { kind: GoogleDriveLocationKind; id?: string; name: string };
type Connection = { id: string; name: string; protocol: Protocol; host: string; port: number; username: string; initialPath: string; keyPath?: string; keyPassphraseNotRequired?: boolean; hostKey?: string; localDirectory?: string; tags: string; s3Region?: string; s3Endpoint?: string; s3ForcePathStyle?: boolean; s3PreserveEmptyDirectories?: boolean; smbShare?: string; smbDomain?: string; smbGuest?: boolean; googleDriveLocationKind?: GoogleDriveLocationKind; googleDriveLocationId?: string; transferMaxConcurrent?: number; transferBandwidthLimitKbps?: number; transferRetryCount?: number };
type ConnectionHistory = { bookmarkId: string; name: string; protocol: Protocol; host: string; port: number; username: string; connectedAt: string };
type Transfer = { id: string; name: string; direction: 'Upload' | 'Download'; status: 'Running' | 'Completed' | 'Failed' | 'Cancelled'; detail: string; activity?: 'queued' | 'running' | 'reconnecting'; localPath?: string; remotePath?: string; connectionId?: string; transferredBytes?: number; totalBytes?: number; speed?: number; etaSeconds?: number; retryCount?: number; conflictPolicy?: string; isDirectory?: boolean };
type SshKey = { name: string; path: string; publicKeyPath?: string; pairedKeyPath?: string; keyType: 'private' | 'public'; kind: string };
type DirectoryProgress = { transferId: string; completedFiles: number; totalFiles: number; currentPath: string; status: string };
type FileProgress = { transferId: string; transferredBytes: number; totalBytes: number; elapsedMs: number; status: string };
type TransferOutcome = { bytes: number; verification: 'sha256' | 'size' };
type LocalPathInfo = { name: string; isDirectory: boolean };
type TransferHistory = { id: string; name: string; direction: 'Upload' | 'Download'; status: 'Completed' | 'Failed' | 'Cancelled'; detail: string; bytes: number; completedAt: string };
type TransferJob = { id: string; connectionId: string; name: string; direction: 'Upload' | 'Download'; localPath: string; remotePath: string; status: 'Failed'; detail: string; transferredBytes: number; totalBytes: number; retryCount: number; conflictPolicy: string; isDirectory: boolean; updatedAt: string };
type Language = 'ja' | 'en' | 'zh-CN';
type Preferences = { language: Language; theme: 'system' | 'light' | 'dark'; fileNameFontSize: number; fileNameFontWeight: 'normal' | 'bold'; fileRowDensity: 'extraCompact' | 'compact' | 'standard' | 'comfortable'; showHiddenFiles: boolean; googleClientId: string; googleDocsExport: 'docx' | 'pdf' | 'odt' | 'txt'; googleSheetsExport: 'xlsx' | 'pdf' | 'csv'; googleSlidesExport: 'pptx' | 'pdf'; googleDrawingsExport: 'pdf' | 'png' | 'svg'; defaultProtocol: Protocol; conflictPolicy: 'ask' | 'overwrite' | 'skip'; maxConcurrentTransfers: number; bandwidthLimitKbps: number; automaticRetryCount: number; confirmDelete: boolean; transferNotifications: boolean; editorPath: string; autoCheckUpdates: boolean };
type GoogleExportPreferences = Pick<Preferences, 'googleDocsExport' | 'googleSheetsExport' | 'googleSlidesExport' | 'googleDrawingsExport'>;
type GoogleAuthorizationStatus = { authorized: boolean; email?: string; clientMatches: boolean; credentialsReady: boolean };
type SoftwareUpdatePhase = 'idle' | 'checking' | 'current' | 'available' | 'downloading' | 'ready' | 'error';
type SoftwareUpdateState = { phase: SoftwareUpdatePhase; currentVersion: string; version?: string; body?: string; downloadedBytes: number; totalBytes?: number; error?: string };
type RemoteEdit = { editId: string; connectionId: string; name: string; remotePath: string; status: 'watching' | 'waiting' | 'failed'; detail?: string };
type RemoteEditOpenResult = { editId: string; name: string; remotePath: string };
type RemoteEditPollResult = { editId: string; remotePath: string; status: 'clean' | 'waiting' | 'uploaded'; bytes: number };
type DragExport = { exportId: string; name: string; remotePath: string; localPath: string; iconPath: string; connectionId: string };
type ColumnKey = 'name' | 'size' | 'modified' | 'permissions' | 'owner' | 'group' | 'type';
type ColumnWidths = Record<ColumnKey, number>;
type OptionalColumnKey = Exclude<ColumnKey, 'name'>;
type ColumnVisibility = Record<OptionalColumnKey, boolean>;
type SortDirection = 'ascending' | 'descending';
type ColumnMenuPosition = { x: number; y: number };
type ViewMode = 'list' | 'icons' | 'columns';
type ColumnLevel = { path: string; entries: FileEntry[]; selectedName?: string };
type SyncDirection = 'localToRemote' | 'remoteToLocal';
type SyncComparison = 'sizeOnly' | 'sizeAndModified';
type SyncAction = 'upload' | 'download' | 'createRemoteDirectory' | 'createLocalDirectory' | 'conflict' | 'destinationOnly';
type SyncPreviewItem = { path: string; action: SyncAction; localSize?: number; remoteSize?: number; isDirectory: boolean };
type SyncPreview = { direction: SyncDirection; items: SyncPreviewItem[]; transferCount: number; directoryCount: number; conflictCount: number; destinationOnlyCount: number };
type SyncConflictChoice = 'skip' | 'source';
type SyncExecutionLogItem = { path: string; action: SyncAction; status: string; detail: string; bytes: number };
type SyncExecutionResult = { syncId: string; status: string; completedItems: number; totalItems: number; bytes: number; log: SyncExecutionLogItem[] };
type SyncExecutionProgress = { syncId: string; completedItems: number; totalItems: number; currentPath: string; status: string };
type SyncHistory = { id: string; direction: SyncDirection; localDirectory: string; remoteDirectory: string; status: string; completedItems: number; totalItems: number; bytes: number; detail: string; completedAt: string };
type BookmarkExportFile = { format: 'harbor-transfer-bookmarks'; version: 1; exportedAt: string; bookmarks: Connection[] };
type EntryContextMenu = { x: number; y: number; entry: FileEntry; basePath: string };
type BrowserContextMenu = { x: number; y: number; basePath: string };
type RemoteClipboard = { connectionId: string; sourcePath: string; entry: FileEntry; mode: 'cut' | 'copy' };
type SelectedRemoteItem = { connectionId: string; remotePath: string; basePath: string; entry: FileEntry };
type FileInformationTarget = { items: SelectedRemoteItem[]; focus: 'name' | 'metadata' };
type InlineRenameTarget = { connectionId: string; remotePath: string; basePath: string; entry: FileEntry; value: string };
type DropConflictChoice = 'cancel' | 'overwrite' | 'merge' | 'replace';
type DropConflictPrompt = {
  name: string;
  incomingIsDirectory: boolean;
  existingIsDirectory: boolean;
  resolve: (choice: DropConflictChoice) => void;
};

const defaultPreferences: Preferences = { language: 'ja', theme: 'system', fileNameFontSize: 13, fileNameFontWeight: 'normal', fileRowDensity: 'standard', showHiddenFiles: true, googleClientId: '', googleDocsExport: 'docx', googleSheetsExport: 'xlsx', googleSlidesExport: 'pptx', googleDrawingsExport: 'pdf', defaultProtocol: 'sftp', conflictPolicy: 'ask', maxConcurrentTransfers: 3, bandwidthLimitKbps: 0, automaticRetryCount: 3, confirmDelete: true, transferNotifications: true, editorPath: '', autoCheckUpdates: true };
function loadPreferences(): Preferences {
  try {
    const saved = { ...defaultPreferences, ...JSON.parse(localStorage.getItem('harbor-transfer.preferences') ?? '{}') };
    const fileRowDensity = saved.fileRowDensity === 'extraCompact' || saved.fileRowDensity === 'compact' || saved.fileRowDensity === 'comfortable' ? saved.fileRowDensity : 'standard';
    return { ...saved, fileNameFontSize: Math.min(20, Math.max(10, Number(saved.fileNameFontSize) || defaultPreferences.fileNameFontSize)), fileNameFontWeight: saved.fileNameFontWeight === 'bold' ? 'bold' : 'normal', fileRowDensity, showHiddenFiles: typeof saved.showHiddenFiles === 'boolean' ? saved.showHiddenFiles : defaultPreferences.showHiddenFiles, googleClientId: typeof saved.googleClientId === 'string' ? saved.googleClientId.trim().slice(0, 512) : '', googleDocsExport: ['docx', 'pdf', 'odt', 'txt'].includes(saved.googleDocsExport) ? saved.googleDocsExport : 'docx', googleSheetsExport: ['xlsx', 'pdf', 'csv'].includes(saved.googleSheetsExport) ? saved.googleSheetsExport : 'xlsx', googleSlidesExport: ['pptx', 'pdf'].includes(saved.googleSlidesExport) ? saved.googleSlidesExport : 'pptx', googleDrawingsExport: ['pdf', 'png', 'svg'].includes(saved.googleDrawingsExport) ? saved.googleDrawingsExport : 'pdf', maxConcurrentTransfers: Math.min(16, Math.max(1, Math.round(Number(saved.maxConcurrentTransfers) || 3))), bandwidthLimitKbps: Math.min(10_485_760, Math.max(0, Math.round(Number(saved.bandwidthLimitKbps) || 0))), automaticRetryCount: saved.automaticRetryCount === undefined ? defaultPreferences.automaticRetryCount : Math.min(10, Math.max(0, Math.round(Number(saved.automaticRetryCount) || 0))) } as Preferences;
  } catch { return defaultPreferences; }
}

function transferLimits(preferences: Preferences, connection: Connection) {
  return {
    globalMaxConcurrentTransfers: preferences.maxConcurrentTransfers,
    connectionMaxConcurrentTransfers: connection.transferMaxConcurrent ?? null,
    globalBandwidthLimitBps: Math.round(preferences.bandwidthLimitKbps * 1024),
    connectionBandwidthLimitBps: connection.transferBandwidthLimitKbps === undefined ? null : Math.round(connection.transferBandwidthLimitKbps * 1024),
    automaticRetryCount: connection.transferRetryCount ?? preferences.automaticRetryCount,
  };
}
async function chooseEditorApplication(): Promise<string | null> {
  const selected = await open({ defaultPath: '/Applications', multiple: false, directory: false, filters: [{ name: 'macOS Applications', extensions: ['app'] }] });
  return selected && !Array.isArray(selected) ? selected : null;
}
const listColumnOrder: ColumnKey[] = ['name', 'size', 'modified', 'permissions', 'owner', 'group', 'type'];
const optionalColumnOrder: OptionalColumnKey[] = ['size', 'modified', 'permissions', 'owner', 'group', 'type'];
const minimumColumnWidths: ColumnWidths = { name: 160, size: 76, modified: 150, permissions: 104, owner: 90, group: 90, type: 84 };
const maximumColumnWidths: ColumnWidths = { name: 1200, size: 240, modified: 360, permissions: 280, owner: 320, group: 320, type: 240 };
const defaultColumnWidths: ColumnWidths = { name: 320, size: 76, modified: 150, permissions: 104, owner: 110, group: 110, type: 92 };
const defaultColumnVisibility: ColumnVisibility = { size: true, modified: true, permissions: true, owner: false, group: false, type: false };
const technicalInputProps = { autoCapitalize: 'none' as const, autoCorrect: 'off' as const, spellCheck: false };
const fileDensityMetrics = {
  extraCompact: { list: 30, column: 26, icon: 120 },
  compact: { list: 36, column: 30, icon: 128 },
  standard: { list: 44, column: 34, icon: 136 },
  comfortable: { list: 52, column: 40, icon: 148 },
} as const;
const defaultSidebarWidth = 244;
const minimumSidebarWidth = 180;
const maximumSidebarWidth = 480;
function clampSidebarWidth(width: number, viewportWidth = window.innerWidth) { return Math.min(Math.max(minimumSidebarWidth, Math.min(maximumSidebarWidth, viewportWidth - 520)), Math.max(minimumSidebarWidth, width)); }
function loadSidebarWidth() { const saved = Number(localStorage.getItem('harbor-transfer.sidebar-width')); return clampSidebarWidth(Number.isFinite(saved) && saved > 0 ? saved : defaultSidebarWidth); }
function loadColumnWidths(): ColumnWidths { try { return { ...defaultColumnWidths, ...JSON.parse(localStorage.getItem('harbor-transfer.column-widths-v2') ?? '{}') }; } catch { return defaultColumnWidths; } }
function loadColumnVisibility(): ColumnVisibility { try { return { ...defaultColumnVisibility, ...JSON.parse(localStorage.getItem('harbor-transfer.column-visibility') ?? '{}') }; } catch { return defaultColumnVisibility; } }

const copy = {
  ja: { title: 'Harbor Transfer', connect: '新規接続', bookmarks: 'ブックマーク', history: '履歴', transfer: '転送', refresh: '更新', upload: 'アップロード', uploadFolder: 'フォルダをアップロード', newFolder: '新規フォルダ', search: '検索', empty: '接続先を選択してください', emptyDetail: '新規接続を作成するか、ブックマークを選択して開始します。', path: 'パス', breadcrumbs: '現在のディレクトリ', copyPath: 'パスをコピー', pathCopied: 'パスをクリップボードにコピーしました', name: '名前', size: 'サイズ', modified: '更新日', status: '転送キュー', connectTitle: '新規接続', editBookmark: 'ブックマークを編集', editBookmarkTitle: 'ブックマークの編集', bookmarkSaved: 'ブックマークを保存しました', saveBookmark: '変更を保存', saveOnly: '保存する', exportBookmarks: 'ブックマークを書き出す', importBookmarks: 'ブックマークを読み込む', bookmarksExported: 'ブックマークを書き出しました', bookmarksImported: '{{count}}件のブックマークを読み込みました', noBookmarksToExport: '書き出すブックマークがありません', invalidBookmarkFile: '有効なHarbor Transferブックマークファイルではありません', bookmarkName: 'ブックマーク名', bookmarkNameHint: '例：本番Webサーバー', initialDirectory: '接続時の初期ディレクトリ', initialDirectoryHint: '例：/var/www/html', cancel: 'キャンセル', pause: '停止', resume: '再開', retry: '再試行', start: '接続する', protocol: 'プロトコル', host: 'サーバー', port: 'ポート', user: 'ユーザー名', password: 'パスワード', key: 'SSH 鍵ファイル（任意）', keyPassphrase: 'SSH 鍵のパスフレーズ（任意）', missingKeyPassphraseConfirm: 'SSH鍵のパスフレーズが入力されていません。暗号化された鍵の場合は接続できません。', continueWithoutPassphrase: 'パスフレーズなしで続行', chooseKey: '鍵を選択', keyFormatHint: '.key、.pem、OpenSSH鍵、PuTTY PPKなどの秘密鍵を選択できます。', invalidPort: 'ポートは1〜65535の範囲で指定してください。', checkingHostKey: 'サーバーのホスト鍵を確認しています…', connectingToServer: 'サーバーへ接続しています…', connectionFailed: '接続できませんでした', keys: 'SSH キー', settings: '環境設定', keyManager: 'SSH キー・マネージャー', keyHint: '鍵の内容は読み込まず、ファイル情報だけを表示します。', useKey: 'この鍵を使用', noKeys: '~/.ssh に利用可能な秘密鍵がありません。', pairedKey: '公開鍵あり', hostKeyChanged: 'サーバーのホスト鍵が保存済みの鍵と一致しません。接続を中止しました。', trustHostKey: 'サーバーのホスト鍵を確認してください。\n\n{{fingerprint}}\n\nこのサーバーを信頼して接続しますか？', error: 'エラー', connected: '接続済み', download: 'ダウンロード' },
  en: { title: 'Harbor Transfer', connect: 'New Connection', bookmarks: 'Bookmarks', history: 'History', transfer: 'Transfers', refresh: 'Refresh', upload: 'Upload', uploadFolder: 'Upload Folder', newFolder: 'New Folder', search: 'Search', empty: 'Choose a connection', emptyDetail: 'Create a new connection or select a bookmark to get started.', path: 'Path', breadcrumbs: 'Current directory', copyPath: 'Copy path', pathCopied: 'Path copied to the clipboard', name: 'Name', size: 'Size', modified: 'Modified', status: 'Transfer Queue', connectTitle: 'New Connection', editBookmark: 'Edit bookmark', editBookmarkTitle: 'Edit Bookmark', bookmarkSaved: 'Bookmark saved', saveBookmark: 'Save Changes', saveOnly: 'Save', exportBookmarks: 'Export bookmarks', importBookmarks: 'Import bookmarks', bookmarksExported: 'Bookmarks exported', bookmarksImported: 'Imported {{count}} bookmarks', noBookmarksToExport: 'There are no bookmarks to export', invalidBookmarkFile: 'This is not a valid Harbor Transfer bookmark file', bookmarkName: 'Bookmark name', bookmarkNameHint: 'e.g. Production Web Server', initialDirectory: 'Initial directory on connect', initialDirectoryHint: 'e.g. /var/www/html', cancel: 'Cancel', pause: 'Pause', resume: 'Resume', retry: 'Retry', start: 'Connect', protocol: 'Protocol', host: 'Server', port: 'Port', user: 'Username', password: 'Password', key: 'SSH key file (optional)', keyPassphrase: 'SSH key passphrase (optional)', missingKeyPassphraseConfirm: 'No SSH key passphrase was entered. An encrypted key cannot connect without it.', continueWithoutPassphrase: 'Continue Without Passphrase', chooseKey: 'Choose Key', keyFormatHint: 'Private keys in .key, .pem, OpenSSH, and PuTTY PPK formats are supported.', invalidPort: 'Enter a port between 1 and 65535.', checkingHostKey: 'Checking the server host key…', connectingToServer: 'Connecting to the server…', connectionFailed: 'Could not connect', keys: 'SSH Keys', settings: 'Preferences', keyManager: 'SSH Key Manager', keyHint: 'Key contents are never read; only file metadata is shown.', useKey: 'Use this key', noKeys: 'No private keys are available in ~/.ssh.', pairedKey: 'Public key found', hostKeyChanged: 'The server host key differs from the saved key. Connection was stopped.', trustHostKey: 'Verify the server host key.\n\n{{fingerprint}}\n\nTrust this server and connect?', error: 'Error', connected: 'Connected', download: 'Download' },
  'zh-CN': { title: 'Harbor Transfer', connect: '新建连接', bookmarks: '书签', history: '历史记录', transfer: '传输', refresh: '刷新', upload: '上传', uploadFolder: '上传文件夹', newFolder: '新建文件夹', search: '搜索', empty: '选择一个连接', emptyDetail: '创建新连接或选择书签以开始使用。', path: '路径', breadcrumbs: '当前目录', copyPath: '复制路径', pathCopied: '路径已复制到剪贴板', name: '名称', size: '大小', modified: '修改日期', status: '传输队列', connectTitle: '新建连接', editBookmark: '编辑书签', editBookmarkTitle: '编辑书签', bookmarkSaved: '书签已保存', saveBookmark: '保存更改', saveOnly: '保存', exportBookmarks: '导出书签', importBookmarks: '导入书签', bookmarksExported: '书签已导出', bookmarksImported: '已导入{{count}}个书签', noBookmarksToExport: '没有可导出的书签', invalidBookmarkFile: '这不是有效的Harbor Transfer书签文件', bookmarkName: '书签名称', bookmarkNameHint: '例如：生产环境 Web 服务器', initialDirectory: '连接时的初始目录', initialDirectoryHint: '例如：/var/www/html', cancel: '取消', pause: '暂停', resume: '继续', retry: '重试', start: '连接', protocol: '协议', host: '服务器', port: '端口', user: '用户名', password: '密码', key: 'SSH 密钥文件（可选）', keyPassphrase: 'SSH 密钥口令（可选）', missingKeyPassphraseConfirm: '尚未输入SSH密钥口令。加密密钥没有口令将无法连接。', continueWithoutPassphrase: '无口令继续', chooseKey: '选择密钥', keyFormatHint: '支持 .key、.pem、OpenSSH 和 PuTTY PPK 格式的私钥。', invalidPort: '请输入1到65535之间的端口。', checkingHostKey: '正在检查服务器主机密钥…', connectingToServer: '正在连接服务器…', connectionFailed: '无法连接', keys: 'SSH 密钥', settings: '偏好设置', keyManager: 'SSH 密钥管理器', keyHint: '不会读取密钥内容，仅显示文件信息。', useKey: '使用此密钥', noKeys: '~/.ssh 中没有可用的私钥。', pairedKey: '已找到公钥', hostKeyChanged: '服务器主机密钥与已保存的密钥不一致，已停止连接。', trustHostKey: '请验证服务器主机密钥。\n\n{{fingerprint}}\n\n信任此服务器并连接吗？', error: '错误', connected: '已连接', download: '下载' },
} as const;

const phaseOneCopy = {
  ja: { tags: 'タグ', all: 'すべて', noHistory: '接続履歴はありません', tagHint: 'カンマ区切り（例: 本番, Web）', webdavHint: '証明書を検証するHTTPS接続だけを使用します。NextcloudではDAVのパスを初期ディレクトリに指定してください。' },
  en: { tags: 'Tags', all: 'All', noHistory: 'No connection history', tagHint: 'Comma separated (e.g. Production, Web)', webdavHint: 'Only HTTPS with certificate verification is used. For Nextcloud, enter the DAV path as the initial directory.' },
  'zh-CN': { tags: '标签', all: '全部', noHistory: '没有连接历史记录', tagHint: '使用逗号分隔（例如：生产, Web）', webdavHint: '仅使用经过证书验证的HTTPS连接。使用Nextcloud时，请将DAV路径填写为初始目录。' },
} as const;

const sshPassphrasePromptCopy = {
  ja: { title: 'SSH鍵のパスフレーズが必要です', detail: '秘密鍵を復号できませんでした。パスフレーズを入力して再接続してください。', withoutPassphrase: 'SSH鍵のパスフレーズなし', withoutPassphraseDetail: 'この秘密鍵が暗号化されていない場合に選択します。', saveInKeychain: 'macOS Keychainに保存', saveInKeychainDetail: '次回から自動入力します。ブックマークや書き出しファイルには保存されません。', keychainLoadFailed: 'macOS Keychainからパスフレーズを読み込めませんでした。', keychainEntryMissing: 'Keychainに保存済みのパスフレーズがありません。パスフレーズを入力してください。', retry: '入力して再接続' },
  en: { title: 'SSH key passphrase required', detail: 'The private key could not be decrypted. Enter its passphrase and reconnect.', withoutPassphrase: 'SSH key has no passphrase', withoutPassphraseDetail: 'Select this when the private key is not encrypted.', saveInKeychain: 'Save in macOS Keychain', saveInKeychainDetail: 'Fills it automatically next time. It is never stored in bookmarks or export files.', keychainLoadFailed: 'The passphrase could not be loaded from macOS Keychain.', keychainEntryMissing: 'No passphrase is saved in Keychain. Enter the passphrase.', retry: 'Enter and Reconnect' },
  'zh-CN': { title: '需要SSH密钥口令', detail: '无法解密私钥。请输入口令并重新连接。', withoutPassphrase: 'SSH密钥没有口令', withoutPassphraseDetail: '私钥未加密时选择此项。', saveInKeychain: '存储到macOS钥匙串', saveInKeychainDetail: '下次将自动填写。不会存储在书签或导出文件中。', keychainLoadFailed: '无法从macOS钥匙串读取口令。', keychainEntryMissing: '钥匙串中没有已保存的口令。请输入口令。', retry: '输入并重新连接' },
} as const;

const windowCopy = {
  ja: { newWindow: '新規ウインドウ', newWindowFailed: '新しいウインドウを開けませんでした' },
  en: { newWindow: 'New Window', newWindowFailed: 'Could not open a new window' },
  'zh-CN': { newWindow: '新建窗口', newWindowFailed: '无法打开新窗口' },
} as const;

const bookmarkLocalCopy = {
  ja: { title: 'ローカルディレクトリ', detail: '差分同期で、この接続先と組み合わせる既定フォルダです。', select: 'フォルダを選択', clear: '解除', none: '選択されていません' },
  en: { title: 'Local directory', detail: 'The default folder paired with this connection for differential sync.', select: 'Choose Folder', clear: 'Clear', none: 'Not selected' },
  'zh-CN': { title: '本地目录', detail: '差异同步时与此连接配对的默认文件夹。', select: '选择文件夹', clear: '清除', none: '未选择' },
} as const;

const s3Copy = {
  ja: { bucket: 'バケット', accessKey: 'Access Key ID', secretKey: 'Secret Access Key', sessionToken: 'Session Token（任意）', region: 'リージョン', endpoint: 'カスタムエンドポイント（任意・HTTPS）', pathStyle: 'パス形式のURLを使用', preserveEmpty: '空フォルダを0 byte markerで保持', readOnly: 'S3ではフォルダをobject keyのprefixとして扱います。空フォルダのmarkerは明示設定時だけ作成します。' },
  en: { bucket: 'Bucket', accessKey: 'Access Key ID', secretKey: 'Secret Access Key', sessionToken: 'Session Token (optional)', region: 'Region', endpoint: 'Custom endpoint (optional, HTTPS)', pathStyle: 'Use path-style URLs', preserveEmpty: 'Preserve empty folders with 0-byte markers', readOnly: 'S3 folders are represented by object-key prefixes. Empty-folder markers are created only when explicitly enabled.' },
  'zh-CN': { bucket: '存储桶', accessKey: 'Access Key ID', secretKey: 'Secret Access Key', sessionToken: 'Session Token（可选）', region: '区域', endpoint: '自定义端点（可选，仅HTTPS）', pathStyle: '使用路径样式 URL', preserveEmpty: '使用0字节标记保留空文件夹', readOnly: 'S3文件夹由对象键前缀表示。仅在明确启用时创建空文件夹标记。' },
} as const;

const sambaCopy = {
  ja: { share: '共有名', shareHint: '例：Documents', domain: 'ワークグループ／ドメイン（任意）', domainHint: '例：WORKGROUP', guest: 'ゲストとして接続', security: 'SMB 2/3を使用します。パスワードはブックマークに含めず、macOS Keychainへ安全に保存します。' },
  en: { share: 'Share name', shareHint: 'e.g. Documents', domain: 'Workgroup / domain (optional)', domainHint: 'e.g. WORKGROUP', guest: 'Connect as guest', security: 'Uses SMB 2/3. Passwords are excluded from bookmarks and stored securely in macOS Keychain.' },
  'zh-CN': { share: '共享名称', shareHint: '例如：Documents', domain: '工作组／域（可选）', domainHint: '例如：WORKGROUP', guest: '以访客身份连接', security: '使用SMB 2/3。密码不会写入书签，而是安全地存储在macOS钥匙串中。' },
} as const;

const googleDriveCopy = {
  ja: {
    tab: 'Google Drive', setupTitle: 'Google Cloudの準備', setupDetail: 'Harbor Transfer側では共通のGCPプロジェクトを使用しません。ご自身のGoogle Cloudプロジェクトでデスクトップアプリ用OAuth Client IDを作成してください。',
    developers: 'Google Developersを開く', project: '1. プロジェクトを作成', api: '2. Google Drive APIを有効化', consent: '3. Google Auth Platformを設定', client: '4. OAuth Client IDを作成',
    stepProject: 'Google Cloud Consoleで新しいプロジェクトを作成します。', stepApi: 'APIライブラリからGoogle Drive APIを有効にします。', stepConsent: 'Google Auth Platformでアプリ名、サポートメール、対象ユーザーを設定し、スコープにGoogle Drive APIを追加します。Externalのテスト運用では自分のGoogleアカウントをテストユーザーに追加してください。', stepClient: 'Clientsから「デスクトップアプリ」を選んでOAuth Client IDを作成します。Webアプリではありません。', stepPaste: 'Google CloudからDesktop Clientのcredentials.jsonを読み込み、Googleアカウントを認証します。Client SecretはKeychainだけに保存します。',
    clientId: 'OAuth Client ID', clientIdHint: '123456789-xxxxx.apps.googleusercontent.com', importCredentials: 'credentials.jsonを読み込む', credentialsReady: 'Client SecretはKeychainに保存されています', credentialsMissing: 'Desktop Clientのcredentials.jsonを読み込んでください。', scopeWarning: 'ファイル一覧・転送・同期にはGoogle Drive全体へのアクセス権を使用します。認証画面でアクセス内容を確認してください。', authorize: 'Googleアカウントを認証', authorizing: 'ブラウザでGoogle認証を完了してください…', disconnect: '認証を解除', connected: '認証済み', notConnected: '未認証', mismatch: '保存済み認証は別のClient ID用です。現在のClient IDで再認証してください。', invalidClientId: 'デスクトップアプリ用OAuth Client IDを入力してください。', connectHint: '先に環境設定のGoogle Driveでcredentials.jsonを読み込み、Googleアカウントを認証してください。', location: '接続先', myDrive: 'マイドライブ', sharedWithMe: '共有アイテム', loadingLocations: '共有ドライブを読み込んでいます…', locationLoadFailed: '共有ドライブの一覧を取得できませんでした。', exportTitle: 'Google形式の書き出し', exportDetail: 'ダウンロード時の形式を選択します。書き出したファイルを再アップロードしても、元のGoogle形式には戻らず別ファイルになります。CSVは先頭のシートだけを書き出します。', documents: 'Google Docs', spreadsheets: 'Google Sheets', presentations: 'Google Slides', drawings: 'Google Drawings', revokeConfirm: 'Keychainに保存したGoogle Drive認証情報を削除しますか？', authFailed: 'Google認証に失敗しました。', openFailed: 'Googleの設定ページを開けませんでした。', nativeExportUnsupported: 'このGoogle形式は選択した書き出し方式に対応していません。'
  },
  en: {
    tab: 'Google Drive', setupTitle: 'Prepare Google Cloud', setupDetail: 'Harbor Transfer does not use a shared GCP project. Create a Desktop OAuth Client ID in your own Google Cloud project.',
    developers: 'Open Google Developers', project: '1. Create a project', api: '2. Enable Google Drive API', consent: '3. Configure Google Auth Platform', client: '4. Create an OAuth Client ID',
    stepProject: 'Create a new project in Google Cloud Console.', stepApi: 'Enable Google Drive API from the API Library.', stepConsent: 'Configure the app name, support email, audience, and Google Drive scope in Google Auth Platform. For External testing, add your Google Account as a test user.', stepClient: 'In Clients, create an OAuth Client ID with application type “Desktop app,” not Web application.', stepPaste: 'Import the Desktop Client credentials.json from Google Cloud, then authorize your Google Account. The Client Secret is stored only in Keychain.',
    clientId: 'OAuth Client ID', clientIdHint: '123456789-xxxxx.apps.googleusercontent.com', importCredentials: 'Import credentials.json', credentialsReady: 'Client Secret is stored in Keychain', credentialsMissing: 'Import the Desktop Client credentials.json.', scopeWarning: 'Browsing, transfers, and sync require access to your entire Google Drive. Review the requested access on Google’s consent screen.', authorize: 'Authorize Google Account', authorizing: 'Complete Google authorization in your browser…', disconnect: 'Remove authorization', connected: 'Authorized', notConnected: 'Not authorized', mismatch: 'The saved authorization belongs to another Client ID. Authorize the current Client ID again.', invalidClientId: 'Enter a Desktop OAuth Client ID.', connectHint: 'Import credentials.json and authorize Google Drive in Preferences before connecting.', location: 'Location', myDrive: 'My Drive', sharedWithMe: 'Shared with me', loadingLocations: 'Loading shared drives…', locationLoadFailed: 'Could not load shared drives.', exportTitle: 'Google format exports', exportDetail: 'Choose download formats. Re-uploading an exported file creates a separate file and does not restore the original Google format. CSV exports only the first sheet.', documents: 'Google Docs', spreadsheets: 'Google Sheets', presentations: 'Google Slides', drawings: 'Google Drawings', revokeConfirm: 'Remove the Google Drive authorization stored in Keychain?', authFailed: 'Google authorization failed.', openFailed: 'Could not open the Google setup page.', nativeExportUnsupported: 'This Google item cannot use the selected export format.'
  },
  'zh-CN': {
    tab: 'Google 云端硬盘', setupTitle: '准备 Google Cloud', setupDetail: 'Harbor Transfer 不使用共享的GCP项目。请在您自己的Google Cloud项目中创建桌面应用OAuth客户端ID。',
    developers: '打开 Google Developers', project: '1. 创建项目', api: '2. 启用 Google Drive API', consent: '3. 配置 Google Auth Platform', client: '4. 创建 OAuth 客户端ID',
    stepProject: '在Google Cloud Console中创建新项目。', stepApi: '从API库启用Google Drive API。', stepConsent: '在Google Auth Platform中设置应用名称、支持邮箱、目标用户和Google Drive权限范围。使用外部测试时，请将自己的Google账号添加为测试用户。', stepClient: '在客户端页面创建应用类型为“桌面应用”的OAuth客户端ID，不要选择Web应用。', stepPaste: '从Google Cloud导入桌面客户端credentials.json，然后授权Google账号。Client Secret仅存储在钥匙串中。',
    clientId: 'OAuth 客户端ID', clientIdHint: '123456789-xxxxx.apps.googleusercontent.com', importCredentials: '导入 credentials.json', credentialsReady: 'Client Secret已存储在钥匙串中', credentialsMissing: '请导入桌面客户端credentials.json。', scopeWarning: '浏览、传输和同步需要访问整个Google云端硬盘。请在Google授权页面确认访问权限。', authorize: '授权 Google 账号', authorizing: '请在浏览器中完成Google授权…', disconnect: '移除授权', connected: '已授权', notConnected: '未授权', mismatch: '保存的授权属于另一个客户端ID。请使用当前客户端ID重新授权。', invalidClientId: '请输入桌面应用OAuth客户端ID。', connectHint: '请先在偏好设置中导入credentials.json并完成Google授权。', location: '连接位置', myDrive: '我的云端硬盘', sharedWithMe: '与我共享', loadingLocations: '正在加载共享云端硬盘…', locationLoadFailed: '无法加载共享云端硬盘。', exportTitle: 'Google格式导出', exportDetail: '选择下载格式。重新上传导出的文件不会恢复为原始Google格式，而会成为单独文件。CSV只导出第一个工作表。', documents: 'Google文档', spreadsheets: 'Google表格', presentations: 'Google幻灯片', drawings: 'Google绘图', revokeConfirm: '要删除存储在钥匙串中的Google云端硬盘授权信息吗？', authFailed: 'Google授权失败。', openFailed: '无法打开Google设置页面。', nativeExportUnsupported: '此Google项目不支持所选导出格式。'
  },
} as const;

const phaseTwoCopy = {
  ja: { conflict: '同名の項目があります。「上書き」「スキップ」「別名」のいずれかを入力してください。', overwrite: '上書き', skip: 'スキップ', rename: '別名', action: '操作を入力してください: edit / download / rename / delete', renameTo: '新しい名前', deleteConfirm: 'この項目を削除しますか？', drop: 'ここにファイルやフォルダをドロップ', speed: '速度', eta: '残り', completed: '転送が完了しました', failed: '転送に失敗しました', cancelled: '転送を取り消しました' },
  en: { conflict: 'An item with this name exists. Enter overwrite, skip, or rename.', overwrite: 'overwrite', skip: 'skip', rename: 'rename', action: 'Enter action: edit / download / rename / delete', renameTo: 'New name', deleteConfirm: 'Delete this item?', drop: 'Drop files or folders here', speed: 'Speed', eta: 'ETA', completed: 'Transfer completed', failed: 'Transfer failed', cancelled: 'Transfer cancelled' },
  'zh-CN': { conflict: '存在同名项目。请输入 overwrite、skip 或 rename。', overwrite: 'overwrite', skip: 'skip', rename: 'rename', action: '输入操作：edit / download / rename / delete', renameTo: '新名称', deleteConfirm: '删除此项目吗？', drop: '将文件或文件夹拖放到这里', speed: '速度', eta: '剩余', completed: '传输完成', failed: '传输失败', cancelled: '传输已取消' },
} as const;

const dropConflictCopy = {
  ja: {
    title: '同名の項目があります', fileDetail: '「{{name}}」はすでに存在します。上書きしますか？', folderDetail: '「{{name}}」フォルダはすでに存在します。更新方法を選択してください。', typeMismatchDetail: '同名の「{{name}}」がありますが、ファイルとフォルダの種類が異なります。既存の項目を置き換えますか？', merge: '差分を統合して上書き', mergeDetail: 'サーバー側だけにある項目は残し、同名ファイルを更新します。', replace: 'フォルダを置き換え', replaceDetail: '既存フォルダを削除し、ドロップしたフォルダだけに置き換えます。', overwrite: '上書き', cancel: 'スキップ' },
  en: {
    title: 'An item with this name already exists', fileDetail: '“{{name}}” already exists. Do you want to overwrite it?', folderDetail: 'The folder “{{name}}” already exists. Choose how to update it.', typeMismatchDetail: 'An item named “{{name}}” exists, but its type differs from the dropped item. Replace the existing item?', merge: 'Merge and overwrite changes', mergeDetail: 'Keep remote-only items and update files with matching names.', replace: 'Replace folder', replaceDetail: 'Delete the existing folder and replace it with only the dropped folder.', overwrite: 'Overwrite', cancel: 'Skip' },
  'zh-CN': {
    title: '存在同名项目', fileDetail: '“{{name}}”已存在。是否覆盖？', folderDetail: '文件夹“{{name}}”已存在。请选择更新方式。', typeMismatchDetail: '存在同名的“{{name}}”，但文件与文件夹类型不同。是否替换现有项目？', merge: '合并差异并覆盖', mergeDetail: '保留仅存在于服务器上的项目，并更新同名文件。', replace: '替换文件夹', replaceDetail: '删除现有文件夹，仅使用拖放的文件夹进行替换。', overwrite: '覆盖', cancel: '跳过' },
} as const;

const contextMenuCopy = {
  ja: { rename: '名前を変更', cut: 'カット', copy: 'コピー', paste: 'ペースト', delete: '削除', download: 'ダウンロード', information: 'ファイル情報を変更', copied: 'リモートクリップボードにコピーしました', cutReady: '移動する項目を選択しました', pasted: 'ペーストしました', title: 'ファイル情報', detail: '名称と現在の情報を確認し、対応する属性を変更します。', selectedCount: '{{count}}項目を選択中', mixed: '複数の値', multipleName: '複数選択ではファイル名を一括変更できません。共通属性だけを変更できます。', fileName: 'ファイル名', kind: '種類', owner: '所有者', group: 'グループ', changeOwnership: '所有者・グループを変更', permissions: 'パーミッション', changePermissions: 'パーミッションを変更', modified: '更新日時', changeModified: '更新日時を変更', unsupported: 'この接続方式ではパーミッションと更新日時の変更に対応していません。名称は変更できます。', cloudFtpSupport: 'Cloud FTPのアクセス権はCloud Storage IAMで管理されます。パーミッション、所有者、グループ、更新日時は変更できません。', ownershipSftp: '所有者・グループの変更はSFTPで数値UID/GIDを指定した場合だけ利用できます。', ftpSupport: 'FTPサーバーがSITE CHMOD／MFMTに対応している場合に変更できます。', smbSupport: 'SMBでは更新日時を変更できます。POSIXパーミッション、所有者、グループは変更できません。', save: '変更を保存', saved: 'ファイル情報を更新しました', chooseField: '名称を変更するか、変更する属性を選択してください。', invalidName: 'ファイル名に「/」、「.」、「..」は使用できません。', invalidOwnership: '所有者とグループは0以上の数値UID/GIDで入力してください。', invalidPermissions: 'パーミッションは0000〜7777の8進数で入力してください。', invalidDate: '有効な更新日時を入力してください。' },
  en: { rename: 'Rename', cut: 'Cut', copy: 'Copy', paste: 'Paste', delete: 'Delete', download: 'Download', information: 'Change File Information', copied: 'Copied to the remote clipboard', cutReady: 'Item selected for moving', pasted: 'Item pasted', title: 'File Information', detail: 'Review the name and current information, then change supported attributes.', selectedCount: '{{count}} items selected', mixed: 'Mixed values', multipleName: 'File names cannot be changed as a group. Only shared attributes can be changed.', fileName: 'File name', kind: 'Kind', owner: 'Owner', group: 'Group', changeOwnership: 'Change owner and group', permissions: 'Permissions', changePermissions: 'Change permissions', modified: 'Modified date', changeModified: 'Change modified date', unsupported: 'This connection type cannot change POSIX permissions or the modified date. The name can still be changed.', cloudFtpSupport: 'Cloud FTP access is controlled by Cloud Storage IAM. Permissions, owner, group, and modified time cannot be changed.', ownershipSftp: 'Owner and group changes require SFTP and numeric UID/GID values.', ftpSupport: 'Changes require SITE CHMOD and MFMT support from the FTP server.', smbSupport: 'SMB can change the modified date. POSIX permissions, owner, and group are not available.', save: 'Save Changes', saved: 'File information updated', chooseField: 'Change the name or choose at least one attribute.', invalidName: 'The file name cannot contain “/” and cannot be “.” or “..”.', invalidOwnership: 'Enter owner and group as non-negative numeric UID/GID values.', invalidPermissions: 'Enter permissions as an octal value from 0000 to 7777.', invalidDate: 'Enter a valid modified date.' },
  'zh-CN': { rename: '重命名', cut: '剪切', copy: '复制', paste: '粘贴', delete: '删除', download: '下载', information: '更改文件信息', copied: '已复制到远程剪贴板', cutReady: '已选择要移动的项目', pasted: '已粘贴项目', title: '文件信息', detail: '查看名称和当前信息，然后更改支持的属性。', selectedCount: '已选择{{count}}个项目', mixed: '多个值', multipleName: '多选时不能批量更改文件名，只能更改共有属性。', fileName: '文件名', kind: '类型', owner: '所有者', group: '组', changeOwnership: '更改所有者和组', permissions: '权限', changePermissions: '更改权限', modified: '修改日期', changeModified: '更改修改日期', unsupported: '此连接类型不能更改POSIX权限或修改日期，但仍可更改名称。', cloudFtpSupport: 'Cloud FTP访问权限由Cloud Storage IAM管理，不能更改权限、所有者、组或修改时间。', ownershipSftp: '更改所有者和组需要使用SFTP并指定数字UID/GID。', ftpSupport: 'FTP服务器需要支持SITE CHMOD和MFMT才能更改。', smbSupport: 'SMB可以更改修改日期，但不能更改POSIX权限、所有者或组。', save: '保存更改', saved: '文件信息已更新', chooseField: '请更改名称或至少选择一个属性。', invalidName: '文件名不能包含“/”，也不能是“.”或“..”。', invalidOwnership: '请使用非负数字UID/GID输入所有者和组。', invalidPermissions: '请输入0000到7777之间的八进制权限值。', invalidDate: '请输入有效的修改日期。' },
} as const;

const cloudFtpCopy = {
  ja: { hint: 'Google Cloud FTP専用。Cloud StorageへSFTPで接続し、SSH公開鍵で認証します。パスワード認証とPOSIX属性変更には対応していません。', keyRequired: 'Cloud FTPでは、登録済み公開鍵と対になるSSH秘密鍵を選択してください。' },
  en: { hint: 'For Google Cloud FTP. Connects to Cloud Storage over SFTP using SSH public-key authentication. Password authentication and POSIX metadata changes are unavailable.', keyRequired: 'Select the SSH private key paired with the public key registered for this Cloud FTP user.' },
  'zh-CN': { hint: '用于Google Cloud FTP。通过SFTP连接Cloud Storage，并使用SSH公钥认证。不支持密码认证和POSIX属性更改。', keyRequired: '请选择与此Cloud FTP用户所注册公钥配对的SSH私钥。' },
} as const;

const browserContextMenuCopy = {
  ja: { newDirectory: '新規ディレクトリ' },
  en: { newDirectory: 'New Directory' },
  'zh-CN': { newDirectory: '新建目录' },
} as const;

const preferencesCopy = {
  ja: { title: '環境設定', detail: 'すべての接続に適用する共通設定です。', general: '一般', appearance: 'アピアランス', theme: 'カラーテーマ', system: 'システム設定', light: 'ライト', dark: 'ダーク', showHiddenFiles: '不可視ファイルを表示', showHiddenFilesDetail: '名前が「.」から始まるファイルとフォルダを表示します。', fileNameWeight: 'ファイル名の太さ', normal: '標準', bold: 'ボールド', fileRowDensity: 'ファイル表示の行間', extraCompact: '最小', compact: '狭い', standard: '標準', comfortable: '広い', fileNameSize: 'ファイル名の文字サイズ', fileNameSizeDetail: 'リスト、アイコン、カラム表示のファイル名に適用されます。', fileNamePreview: 'ファイル名の表示サンプル.txt', transfers: '転送', security: '安全性', updates: 'アップデート', editorTab: 'エディタ', editor: 'リモートファイルエディタ', editorDetail: 'キャッシュを開くアプリケーションです。保存を検知すると、同名のリモートファイルを自動的に上書きします。', chooseEditor: 'エディタを選択', clearEditor: '解除', noEditor: '選択されていません', language: '表示言語', defaultProtocol: '新規接続の既定プロトコル', conflictPolicy: '同名ファイルの既定動作', ask: '毎回確認', overwrite: '上書き', skip: 'スキップ', confirmDelete: '削除前に確認する', notifications: '転送結果を画面内に通知する', save: '保存' },
  en: { title: 'Preferences', detail: 'These settings apply to every connection.', general: 'General', appearance: 'Appearance', theme: 'Color theme', system: 'System', light: 'Light', dark: 'Dark', showHiddenFiles: 'Show hidden files', showHiddenFilesDetail: 'Shows files and folders whose names begin with a period.', fileNameWeight: 'File name weight', normal: 'Regular', bold: 'Bold', fileRowDensity: 'File display spacing', extraCompact: 'Extra Compact', compact: 'Compact', standard: 'Standard', comfortable: 'Comfortable', fileNameSize: 'File name text size', fileNameSizeDetail: 'Applied to file names in list, icon, and column views.', fileNamePreview: 'File name preview.txt', transfers: 'Transfers', security: 'Safety', updates: 'Updates', editorTab: 'Editor', editor: 'Remote File Editor', editorDetail: 'This application opens cached copies. Saving automatically overwrites the file at the same remote path.', chooseEditor: 'Choose Editor', clearEditor: 'Clear', noEditor: 'Not selected', language: 'Display language', defaultProtocol: 'Default protocol for new connections', conflictPolicy: 'Default duplicate-file action', ask: 'Ask every time', overwrite: 'Overwrite', skip: 'Skip', confirmDelete: 'Confirm before deleting', notifications: 'Show in-app transfer notifications', save: 'Save' },
  'zh-CN': { title: '偏好设置', detail: '这些设置适用于所有连接。', general: '通用', appearance: '外观', theme: '颜色主题', system: '跟随系统', light: '浅色', dark: '深色', showHiddenFiles: '显示隐藏文件', showHiddenFilesDetail: '显示名称以“.”开头的文件和文件夹。', fileNameWeight: '文件名字重', normal: '常规', bold: '粗体', fileRowDensity: '文件显示行距', extraCompact: '最紧凑', compact: '紧凑', standard: '标准', comfortable: '宽松', fileNameSize: '文件名文字大小', fileNameSizeDetail: '应用于列表、图标和分栏视图中的文件名。', fileNamePreview: '文件名显示示例.txt', transfers: '传输', security: '安全性', updates: '软件更新', editorTab: '编辑器', editor: '远程文件编辑器', editorDetail: '此应用用于打开缓存副本。保存后会自动覆盖同一路径下的远程文件。', chooseEditor: '选择编辑器', clearEditor: '清除', noEditor: '未选择', language: '显示语言', defaultProtocol: '新连接的默认协议', conflictPolicy: '同名文件的默认操作', ask: '每次询问', overwrite: '覆盖', skip: '跳过', confirmDelete: '删除前确认', notifications: '在应用内显示传输结果通知', save: '保存' },
} as const;

const transferSettingsCopy = {
  ja: { title: '転送制御', globalDetail: 'すべての接続で共有する上限です。帯域を0にすると無制限になります。', bookmarkDetail: '空欄の場合は環境設定の共通値を使用します。0回は自動再試行を無効にし、帯域の0 KB/sは無制限です。', concurrent: '同時転送数', bandwidth: '帯域上限（KB/s）', retries: '自動再試行回数', inherit: '共通設定を使用' },
  en: { title: 'Transfer Control', globalDetail: 'These limits are shared by all connections. Set bandwidth to 0 for unlimited.', bookmarkDetail: 'Leave a field blank to inherit Preferences. Zero retries disables automatic retry; 0 KB/s means unlimited.', concurrent: 'Concurrent transfers', bandwidth: 'Bandwidth limit (KB/s)', retries: 'Automatic retry attempts', inherit: 'Use global setting' },
  'zh-CN': { title: '传输控制', globalDetail: '这些上限由所有连接共享。带宽设为0表示不限速。', bookmarkDetail: '留空时使用偏好设置中的全局值。重试次数为0时禁用自动重试；带宽0 KB/s表示不限速。', concurrent: '并发传输数', bandwidth: '带宽上限（KB/s）', retries: '自动重试次数', inherit: '使用全局设置' },
} as const;

const softwareUpdateCopy = {
  ja: { title: 'ソフトウェアアップデート', detail: '署名を検証したHarbor Transferの正式リリースだけをインストールします。', automatic: '起動時にアップデートを自動確認する', currentVersion: '現在のバージョン', check: 'アップデートを確認', checking: '確認しています…', current: '最新バージョンです', available: 'バージョン {{version}}を利用できます', details: '詳細を表示', download: 'ダウンロードしてインストール', downloading: 'ダウンロード中', ready: 'インストールが完了しました。再起動して適用してください。', restart: '再起動して更新', later: '後で', failed: 'アップデートを確認できませんでした', releaseNotes: 'リリースノート' },
  en: { title: 'Software Update', detail: 'Only official Harbor Transfer releases with a valid update signature are installed.', automatic: 'Automatically check for updates at launch', currentVersion: 'Current version', check: 'Check for Updates', checking: 'Checking…', current: 'Harbor Transfer is up to date', available: 'Version {{version}} is available', details: 'Show Details', download: 'Download and Install', downloading: 'Downloading', ready: 'Installation is complete. Restart to apply the update.', restart: 'Restart and Update', later: 'Later', failed: 'Unable to check for updates', releaseNotes: 'Release Notes' },
  'zh-CN': { title: '软件更新', detail: '仅安装通过更新签名验证的Harbor Transfer正式版本。', automatic: '启动时自动检查更新', currentVersion: '当前版本', check: '检查更新', checking: '正在检查…', current: 'Harbor Transfer已是最新版本', available: '版本{{version}}可用', details: '显示详情', download: '下载并安装', downloading: '正在下载', ready: '安装已完成。请重新启动以应用更新。', restart: '重新启动并更新', later: '稍后', failed: '无法检查更新', releaseNotes: '发行说明' },
} as const;

const accessibilityCopy = {
  ja: { back: '戻る', forward: '進む', parent: '親フォルダ', more: 'その他の操作' },
  en: { back: 'Back', forward: 'Forward', parent: 'Parent folder', more: 'More actions' },
  'zh-CN': { back: '后退', forward: '前进', parent: '上级文件夹', more: '更多操作' },
} as const;

const sshKeyCopy = {
  ja: { privateKey: '秘密鍵', publicKey: '公開鍵', pairedPublic: '対応する公開鍵あり', pairedPrivate: '対応する秘密鍵あり', usePrivate: '接続に使用', publicInfo: '公開鍵は接続認証には選択できません', noKeys: '~/.ssh に利用可能なSSH鍵がありません。' },
  en: { privateKey: 'Private key', publicKey: 'Public key', pairedPublic: 'Matching public key found', pairedPrivate: 'Matching private key found', usePrivate: 'Use for Connection', publicInfo: 'Public keys cannot be selected for connection authentication', noKeys: 'No SSH keys are available in ~/.ssh.' },
  'zh-CN': { privateKey: '私钥', publicKey: '公钥', pairedPublic: '已找到对应的公钥', pairedPrivate: '已找到对应的私钥', usePrivate: '用于连接', publicInfo: '公钥不能用于连接身份验证', noKeys: '~/.ssh 中没有可用的SSH密钥。' },
} as const;

const remoteEditCopy = {
  ja: { edit: '編集', configure: '編集に使用するアプリケーションを選択してください。', opening: '編集用キャッシュを準備しています', watching: '保存を監視中', waiting: '変更の安定を待っています', uploaded: 'リモートファイルを上書き保存しました', stop: '編集を終了してキャッシュを削除' },
  en: { edit: 'Edit', configure: 'Choose an application to edit remote files.', opening: 'Preparing the editing cache', watching: 'Watching for saves', waiting: 'Waiting for changes to settle', uploaded: 'Remote file overwritten with saved changes', stop: 'Stop editing and delete cache' },
  'zh-CN': { edit: '编辑', configure: '请选择用于编辑远程文件的应用程序。', opening: '正在准备编辑缓存', watching: '正在监视保存操作', waiting: '正在等待更改稳定', uploaded: '已用保存的更改覆盖远程文件', stop: '结束编辑并删除缓存' },
} as const;

const dragOutCopy = {
  ja: { preparing: 'ドラッグ用ファイルを準備しています', ready: 'Finderへドラッグできます', retry: '準備中です。完了後にもう一度ドラッグしてください。', copied: 'Finderへファイルをコピーしました', cancelled: 'ファイルのドラッグを取り消しました' },
  en: { preparing: 'Preparing item for dragging', ready: 'Ready to drag to Finder', retry: 'The item is still being prepared. Drag it again when ready.', copied: 'Item copied to Finder', cancelled: 'Item drag cancelled' },
  'zh-CN': { preparing: '正在准备拖放文件', ready: '可以拖到访达', retry: '文件仍在准备中。完成后请再次拖动。', copied: '文件已复制到访达', cancelled: '已取消文件拖动' },
} as const;

const bookmarkOrderCopy = {
  ja: { handle: '「{{name}}」の順番を変更', hint: 'ドラッグ、または上下矢印キーで順番を変更', filtered: '「すべて」を選択すると順番を変更できます', saved: 'ブックマークの順番を保存しました' },
  en: { handle: 'Reorder “{{name}}”', hint: 'Drag or use the Up and Down Arrow keys to reorder', filtered: 'Select All to reorder bookmarks', saved: 'Bookmark order saved' },
  'zh-CN': { handle: '调整“{{name}}”的顺序', hint: '拖动或使用上下方向键调整顺序', filtered: '选择“全部”后可调整书签顺序', saved: '已保存书签顺序' },
} as const;

const queueCopy = {
  ja: { collapse: '転送キューを折りたたむ', expand: '転送キューを展開', hideSidebar: 'サイドメニューを隠す', showSidebar: 'サイドメニューを表示', resizeSidebar: 'サイドメニューの幅を変更', clearConnectionHistory: '接続履歴を削除', clearTransferHistory: '完了・失敗した転送履歴を削除', confirmConnection: '接続履歴をすべて削除しますか？', confirmTransfer: '完了・失敗・取消済みの転送履歴をすべて削除しますか？', interrupted: 'アプリの終了により転送が中断されました。接続後に再試行できます。', reconnectToRetry: '元の接続先へ接続すると再試行できます', queued: '転送開始を待っています…', reconnecting: '一時的な通信障害のため再接続しています…' },
  en: { collapse: 'Collapse transfer queue', expand: 'Expand transfer queue', hideSidebar: 'Hide sidebar', showSidebar: 'Show sidebar', resizeSidebar: 'Resize sidebar width', clearConnectionHistory: 'Clear connection history', clearTransferHistory: 'Clear completed and failed transfers', confirmConnection: 'Clear all connection history?', confirmTransfer: 'Clear all completed, failed, and cancelled transfer history?', interrupted: 'The transfer was interrupted when the app closed. Reconnect to retry it.', reconnectToRetry: 'Reconnect to the original destination to retry', queued: 'Waiting to start transfer…', reconnecting: 'Temporary network failure. Reconnecting…' },
  'zh-CN': { collapse: '折叠传输队列', expand: '展开传输队列', hideSidebar: '隐藏侧边栏', showSidebar: '显示侧边栏', resizeSidebar: '调整侧边栏宽度', clearConnectionHistory: '清除连接历史记录', clearTransferHistory: '清除已完成和失败的传输', confirmConnection: '清除所有连接历史记录吗？', confirmTransfer: '清除所有已完成、失败和取消的传输历史记录吗？', interrupted: '应用关闭时传输已中断。重新连接后可以重试。', reconnectToRetry: '连接到原始目标后即可重试', queued: '正在等待开始传输…', reconnecting: '发生临时网络故障，正在重新连接…' },
} as const;

const columnCopy = {
  ja: { permissions: 'パーミッション', owner: 'オーナー', group: 'グループ', type: '種類', file: 'ファイル', directory: 'フォルダ', symlink: 'シンボリックリンク', resize: '列幅を変更', displayColumns: '表示する項目', ascending: '昇順', descending: '降順' },
  en: { permissions: 'Permissions', owner: 'Owner', group: 'Group', type: 'Kind', file: 'File', directory: 'Folder', symlink: 'Symbolic link', resize: 'Resize column', displayColumns: 'Show Columns', ascending: 'Ascending', descending: 'Descending' },
  'zh-CN': { permissions: '权限', owner: '所有者', group: '组', type: '类型', file: '文件', directory: '文件夹', symlink: '符号链接', resize: '调整列宽', displayColumns: '显示项目', ascending: '升序', descending: '降序' },
} as const;

const viewCopy = {
  ja: { list: 'リスト表示', icons: 'アイコン表示', columns: 'カラム表示', empty: '項目がありません' },
  en: { list: 'List view', icons: 'Icon view', columns: 'Column view', empty: 'No items' },
  'zh-CN': { list: '列表视图', icons: '图标视图', columns: '分栏视图', empty: '没有项目' },
} as const;

const syncCopy = {
  ja: { button: '同期', title: '片方向同期', detail: '差分と競合を確認してから、安全な同期を実行します。同期先のみの項目は削除しません。', direction: '同期方向', localToRemote: 'ローカル → リモート', remoteToLocal: 'リモート → ローカル', refresh: '差分を再計算', transfers: '転送', directories: 'フォルダ作成', conflicts: '競合', destinationOnly: '同期先のみ', noChanges: '同期が必要な差分はありません。', upload: 'アップロード', download: 'ダウンロード', createRemoteDirectory: 'リモートにフォルダ作成', createLocalDirectory: 'ローカルにフォルダ作成', conflict: '競合', destinationOnlyAction: '同期先のみ（保持）', exclusions: '除外パターン', exclusionHint: '1行に1つ（例：.DS_Store、node_modules/**）', skip: 'スキップ', useSource: '同期元で上書き', execute: '同期を実行', executing: '同期中', confirmExecute: '{{count}}件の変更を実行します。同期先のみの項目は削除されません。続けますか？', cancelExecution: '同期を取り消す', result: '実行結果', history: '同期履歴', clearHistory: '同期履歴を削除', noHistory: '同期履歴はありません', completed: '完了', failed: '失敗', cancelled: '取消済み' },
  en: { button: 'Sync', title: 'One-way Sync', detail: 'Review differences and conflicts before running a safe sync. Destination-only items are never deleted.', direction: 'Direction', localToRemote: 'Local → Remote', remoteToLocal: 'Remote → Local', refresh: 'Recalculate', transfers: 'Transfers', directories: 'Directories', conflicts: 'Conflicts', destinationOnly: 'Destination only', noChanges: 'No differences require synchronization.', upload: 'Upload', download: 'Download', createRemoteDirectory: 'Create remote directory', createLocalDirectory: 'Create local directory', conflict: 'Conflict', destinationOnlyAction: 'Destination only (keep)', exclusions: 'Exclusion patterns', exclusionHint: 'One per line (for example: .DS_Store or node_modules/**)', skip: 'Skip', useSource: 'Overwrite from source', execute: 'Run Sync', executing: 'Syncing', confirmExecute: 'Apply {{count}} changes? Destination-only items will not be deleted.', cancelExecution: 'Cancel sync', result: 'Execution result', history: 'Sync history', clearHistory: 'Clear sync history', noHistory: 'No sync history', completed: 'Completed', failed: 'Failed', cancelled: 'Cancelled' },
  'zh-CN': { button: '同步', title: '单向同步', detail: '确认差异和冲突后再安全执行同步。不会删除仅存在于目标端的项目。', direction: '同步方向', localToRemote: '本地 → 远程', remoteToLocal: '远程 → 本地', refresh: '重新计算', transfers: '传输', directories: '创建文件夹', conflicts: '冲突', destinationOnly: '仅目标端', noChanges: '没有需要同步的差异。', upload: '上传', download: '下载', createRemoteDirectory: '创建远程文件夹', createLocalDirectory: '创建本地文件夹', conflict: '冲突', destinationOnlyAction: '仅目标端（保留）', exclusions: '排除模式', exclusionHint: '每行一个（例如：.DS_Store、node_modules/**）', skip: '跳过', useSource: '使用源文件覆盖', execute: '执行同步', executing: '正在同步', confirmExecute: '将执行{{count}}项更改。不会删除仅存在于目标端的项目。是否继续？', cancelExecution: '取消同步', result: '执行结果', history: '同步历史', clearHistory: '清除同步历史', noHistory: '没有同步历史', completed: '已完成', failed: '失败', cancelled: '已取消' },
} as const;

const syncComparisonCopy = {
  ja: { comparison: '比較方法', sizeOnly: 'サイズのみ（安定）', sizeAndModified: 'サイズ＋更新日時（rsync型）' },
  en: { comparison: 'Comparison', sizeOnly: 'Size only (stable)', sizeAndModified: 'Size + modified time (rsync)' },
  'zh-CN': { comparison: '比较方法', sizeOnly: '仅比较大小（稳定）', sizeAndModified: '大小＋修改时间（rsync）' },
} as const;

const syncUiCopy = {
  ja: { ...syncCopy.ja, ...syncComparisonCopy.ja },
  en: { ...syncCopy.en, ...syncComparisonCopy.en },
  'zh-CN': { ...syncCopy['zh-CN'], ...syncComparisonCopy['zh-CN'] },
} as const;

function joinPath(base: string, name: string) { return base === '/' ? `/${name}` : `${base.replace(/\/$/, '')}/${name}`; }
function entryIdentity(entry: FileEntry) { return entry.path_component ?? entry.name; }
function entryRemotePath(base: string, entry: FileEntry) { return joinPath(base, entryIdentity(entry)); }
function visiblePathComponent(component: string) {
  if (component.startsWith('~gdrive~')) {
    try {
      const encoded = component.slice('~gdrive~'.length, component.lastIndexOf('~'));
      const base64 = encoded.replace(/-/g, '+').replace(/_/g, '/').padEnd(Math.ceil(encoded.length / 4) * 4, '=');
      return new TextDecoder().decode(Uint8Array.from(atob(base64), (character) => character.charCodeAt(0)));
    } catch { return component; }
  }
  return component.includes('\u001f') ? component.slice(0, component.lastIndexOf('\u001f')) : component;
}
function visibleRemotePath(path: string) { return path === '/' ? '/' : `/${path.split('/').filter(Boolean).map(visiblePathComponent).join('/')}`; }
function localDownloadName(entry: FileEntry) { return (entry.download_name ?? entry.name).replaceAll('/', '／').replaceAll(':', '：'); }
function isSshProtocol(protocol: Protocol) { return protocol === 'sftp' || protocol === 'cloudFtp'; }
function protocolLabel(protocol: Protocol) { return protocol === 'cloudFtp' ? 'CLOUD FTP' : protocol.toUpperCase(); }
function defaultPort(protocol: Protocol) { return isSshProtocol(protocol) ? 22 : protocol === 'smb' ? 445 : protocol === 'webdav' || protocol === 's3' || protocol === 'googleDrive' ? 443 : 21; }
function connectionTargetChanged(left: Connection, right: Connection) { return left.protocol !== right.protocol || left.host !== right.host || left.port !== right.port || left.username !== right.username || left.s3Region !== right.s3Region || left.s3Endpoint !== right.s3Endpoint || Boolean(left.s3ForcePathStyle) !== Boolean(right.s3ForcePathStyle) || Boolean(left.s3PreserveEmptyDirectories) !== Boolean(right.s3PreserveEmptyDirectories) || left.smbShare !== right.smbShare || left.smbDomain !== right.smbDomain || Boolean(left.smbGuest) !== Boolean(right.smbGuest); }
function parentPath(path: string) { const parts = path.split('/').filter(Boolean); parts.pop(); return `/${parts.join('/')}` || '/'; }
function formatBytes(bytes: number) { if (!bytes) return '—'; const units = ['B', 'KB', 'MB', 'GB']; const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), 3); return `${(bytes / 1024 ** index).toFixed(index ? 1 : 0)} ${units[index]}`; }
function formatDuration(seconds?: number) { if (seconds === undefined || !Number.isFinite(seconds)) return '—'; if (seconds < 60) return `${Math.ceil(seconds)}s`; return `${Math.floor(seconds / 60)}m ${Math.ceil(seconds % 60)}s`; }
type FileIconCategory = 'image' | 'video' | 'audio' | 'archive' | 'code' | 'data' | 'spreadsheet' | 'document' | 'generic';
function fileIconCategory(name: string): FileIconCategory {
  const lowerName = name.toLowerCase();
  if (/^(readme|license|changelog|authors)(\..*)?$/.test(lowerName)) return 'document';
  if (['makefile', 'dockerfile', 'gemfile', 'rakefile', 'cmakelists.txt'].includes(lowerName)) return 'code';
  const extension = name.includes('.') ? name.split('.').pop()?.toLowerCase() ?? '' : '';
  if (['jpg', 'jpeg', 'png', 'gif', 'webp', 'svg', 'heic', 'heif', 'tif', 'tiff', 'bmp', 'ico', 'raw', 'psd', 'ai'].includes(extension)) return 'image';
  if (['mp4', 'mov', 'm4v', 'avi', 'mkv', 'webm', 'mpg', 'mpeg', 'wmv'].includes(extension)) return 'video';
  if (['mp3', 'wav', 'aac', 'flac', 'ogg', 'oga', 'm4a', 'aiff', 'wma'].includes(extension)) return 'audio';
  if (['zip', 'tar', 'gz', 'tgz', 'bz2', 'xz', '7z', 'rar', 'dmg', 'pkg', 'iso'].includes(extension)) return 'archive';
  if (['html', 'htm', 'css', 'scss', 'sass', 'less', 'js', 'jsx', 'mjs', 'cjs', 'ts', 'tsx', 'php', 'py', 'rb', 'rs', 'go', 'java', 'kt', 'c', 'cc', 'cpp', 'h', 'hpp', 'swift', 'sh', 'bash', 'zsh', 'vue', 'svelte', 'sql'].includes(extension)) return 'code';
  if (['json', 'json5', 'yaml', 'yml', 'toml', 'xml', 'plist', 'env', 'ini', 'conf', 'config', 'lock'].includes(extension)) return 'data';
  if (['csv', 'tsv', 'xls', 'xlsx', 'xlsm', 'numbers', 'ods'].includes(extension)) return 'spreadsheet';
  if (['txt', 'md', 'markdown', 'rtf', 'pdf', 'doc', 'docx', 'pages', 'odt', 'epub'].includes(extension)) return 'document';
  return 'generic';
}
function RemoteEntryIcon({ entry, size, className = '' }: { entry: FileEntry; size: number; className?: string }) {
  if (entry.file_type === 'Directory') return <Folder className={`${className} folder-art remote-entry-icon`} fill="currentColor" size={size}/>;
  if (entry.file_type === 'Symlink') return <Link2 className={`${className} file-type-icon file-type-link`} size={size}/>;
  const category = fileIconCategory(entry.name);
  const icons = { image: FileImage, video: FileVideo, audio: FileAudio, archive: FileArchive, code: FileCode2, data: FileJson, spreadsheet: FileSpreadsheet, document: FileText, generic: File };
  const Icon = icons[category];
  return <Icon className={`${className} file-type-icon file-type-${category}`} size={size}/>;
}
function transferFailureStatus(reason: unknown): 'Failed' | 'Cancelled' { return String(reason).toLowerCase().includes('cancel') ? 'Cancelled' : 'Failed'; }
function invokeErrorMessage(reason: unknown): string {
  if (typeof reason === 'string') return reason;
  if (reason instanceof Error) return reason.message;
  if (reason && typeof reason === 'object' && 'message' in reason && typeof reason.message === 'string') return reason.message;
  try { return JSON.stringify(reason); } catch { return String(reason); }
}
function permissionStringToOctal(value?: string) {
  if (!value) return '';
  const body = value.replace(/[+.@]$/, '').slice(-9);
  if (!/^[rwx-]{9}$/.test(body)) return '';
  return [0, 3, 6].map((offset) => String((body[offset] === 'r' ? 4 : 0) + (body[offset + 1] === 'w' ? 2 : 0) + (body[offset + 2] === 'x' ? 1 : 0))).join('');
}
function toDateTimeLocal(value?: string) {
  if (!value) return '';
  if (/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}/.test(value)) return value.slice(0, 16).replace(' ', 'T');
  const numeric = Number(value);
  const date = Number.isFinite(numeric) && value.trim() !== '' ? new Date(numeric * 1000) : new Date(value);
  if (Number.isNaN(date.getTime())) return '';
  const local = new Date(date.getTime() - date.getTimezoneOffset() * 60_000);
  return local.toISOString().slice(0, 16);
}
function nextCopyName(name: string, isDirectory: boolean, occupied: Set<string>) {
  const dot = isDirectory ? -1 : name.lastIndexOf('.');
  const stem = dot > 0 ? name.slice(0, dot) : name;
  const extension = dot > 0 ? name.slice(dot) : '';
  for (let index = 1; index < 10_000; index += 1) {
    const suffix = index === 1 ? ' copy' : ` copy ${index}`;
    const candidate = `${stem}${suffix}${extension}`;
    if (!occupied.has(candidate)) return candidate;
  }
  return `${stem} copy-${crypto.randomUUID()}${extension}`;
}

function parseBookmarkExport(raw: string): Connection[] | null {
  if (raw.length > 5_000_000) return null;
  try {
    const value = JSON.parse(raw) as Record<string, unknown>;
    if (value.format !== 'harbor-transfer-bookmarks' || value.version !== 1 || !Array.isArray(value.bookmarks) || value.bookmarks.length > 10_000) return null;
    const imported = new Map<string, Connection>();
    for (const item of value.bookmarks) {
      if (!item || typeof item !== 'object') return null;
      const bookmark = item as Record<string, unknown>;
      const protocol = bookmark.protocol;
      const port = bookmark.port;
      const requiredStrings = ['id', 'name', 'host', 'username', 'initialPath', 'tags'] as const;
      if (!requiredStrings.every((key) => typeof bookmark[key] === 'string' && (bookmark[key] as string).length <= 4096)) return null;
      if (!(protocol === 'sftp' || protocol === 'cloudFtp' || protocol === 'ftp' || protocol === 'ftps' || protocol === 'webdav' || protocol === 's3' || protocol === 'smb' || protocol === 'googleDrive') || !Number.isInteger(port) || (port as number) < 1 || (port as number) > 65535) return null;
      if (!(bookmark.keyPath === undefined || (typeof bookmark.keyPath === 'string' && bookmark.keyPath.length <= 4096)) || !(bookmark.hostKey === undefined || (typeof bookmark.hostKey === 'string' && bookmark.hostKey.length <= 4096)) || !(bookmark.localDirectory === undefined || (typeof bookmark.localDirectory === 'string' && bookmark.localDirectory.length <= 4096))) return null;
      if (!(bookmark.keyPassphraseNotRequired === undefined || typeof bookmark.keyPassphraseNotRequired === 'boolean')) return null;
      if (!(bookmark.smbShare === undefined || (typeof bookmark.smbShare === 'string' && bookmark.smbShare.length <= 255)) || !(bookmark.smbDomain === undefined || (typeof bookmark.smbDomain === 'string' && bookmark.smbDomain.length <= 255)) || !(bookmark.smbGuest === undefined || typeof bookmark.smbGuest === 'boolean')) return null;
      if (!(bookmark.googleDriveLocationKind === undefined || bookmark.googleDriveLocationKind === 'myDrive' || bookmark.googleDriveLocationKind === 'sharedWithMe' || bookmark.googleDriveLocationKind === 'sharedDrive')) return null;
      if (!(bookmark.googleDriveLocationId === undefined || (typeof bookmark.googleDriveLocationId === 'string' && bookmark.googleDriveLocationId.length <= 512))) return null;
      if (!(bookmark.transferMaxConcurrent === undefined || (Number.isInteger(bookmark.transferMaxConcurrent) && (bookmark.transferMaxConcurrent as number) >= 1 && (bookmark.transferMaxConcurrent as number) <= 16))) return null;
      if (!(bookmark.transferBandwidthLimitKbps === undefined || (Number.isInteger(bookmark.transferBandwidthLimitKbps) && (bookmark.transferBandwidthLimitKbps as number) >= 0 && (bookmark.transferBandwidthLimitKbps as number) <= 10_485_760))) return null;
      if (!(bookmark.transferRetryCount === undefined || (Number.isInteger(bookmark.transferRetryCount) && (bookmark.transferRetryCount as number) >= 0 && (bookmark.transferRetryCount as number) <= 10))) return null;
      const id = (bookmark.id as string).trim();
      const host = (bookmark.host as string).trim();
      const username = (bookmark.username as string).trim();
      if (!id || !host || (protocol !== 's3' && protocol !== 'googleDrive' && !(protocol === 'smb' && bookmark.smbGuest === true) && !username) || (protocol === 'smb' && !(typeof bookmark.smbShare === 'string' && bookmark.smbShare.trim()))) return null;
      imported.set(id, {
        id,
        name: (bookmark.name as string).trim() || host,
        protocol,
        host,
        port: port as number,
        username,
        initialPath: (bookmark.initialPath as string).trim() || '/',
        keyPath: typeof bookmark.keyPath === 'string' && bookmark.keyPath ? bookmark.keyPath : undefined,
        keyPassphraseNotRequired: bookmark.keyPassphraseNotRequired === true,
        hostKey: typeof bookmark.hostKey === 'string' && bookmark.hostKey ? bookmark.hostKey : undefined,
        localDirectory: typeof bookmark.localDirectory === 'string' && bookmark.localDirectory ? bookmark.localDirectory : undefined,
        tags: bookmark.tags as string,
        s3Region: typeof bookmark.s3Region === 'string' ? bookmark.s3Region : undefined,
        s3Endpoint: typeof bookmark.s3Endpoint === 'string' ? bookmark.s3Endpoint : undefined,
        s3ForcePathStyle: bookmark.s3ForcePathStyle === true,
        s3PreserveEmptyDirectories: bookmark.s3PreserveEmptyDirectories === true,
        smbShare: typeof bookmark.smbShare === 'string' ? bookmark.smbShare.trim() : undefined,
        smbDomain: typeof bookmark.smbDomain === 'string' ? bookmark.smbDomain.trim() : undefined,
        smbGuest: bookmark.smbGuest === true,
        googleDriveLocationKind: bookmark.googleDriveLocationKind as GoogleDriveLocationKind | undefined,
        googleDriveLocationId: typeof bookmark.googleDriveLocationId === 'string' ? bookmark.googleDriveLocationId : undefined,
        transferMaxConcurrent: typeof bookmark.transferMaxConcurrent === 'number' ? bookmark.transferMaxConcurrent : undefined,
        transferBandwidthLimitKbps: typeof bookmark.transferBandwidthLimitKbps === 'number' ? bookmark.transferBandwidthLimitKbps : undefined,
        transferRetryCount: typeof bookmark.transferRetryCount === 'number' ? bookmark.transferRetryCount : undefined,
      });
    }
    return [...imported.values()];
  } catch {
    return null;
  }
}

function ResizableColumnHeader({ label, column, width, resizeLabel, sorted, direction, onSort, onStart, onAdjust }: { label: string; column: ColumnKey; width: number; resizeLabel: string; sorted: boolean; direction: SortDirection; onSort: (column: ColumnKey) => void; onStart: (event: React.PointerEvent, column: ColumnKey) => void; onAdjust: (column: ColumnKey, delta: number) => void }) {
  return <span className="column-heading" role="columnheader" aria-sort={sorted ? direction : 'none'}><button type="button" className="column-sort-button" onClick={() => onSort(column)}><span>{label}</span>{sorted && (direction === 'ascending' ? <ChevronUp size={13}/> : <ChevronDown size={13}/>)}</button><span className="column-resizer" role="separator" aria-label={`${resizeLabel}: ${label}`} aria-orientation="vertical" aria-valuenow={width} tabIndex={0} onPointerDown={(event) => { event.stopPropagation(); onStart(event, column); }} onClick={(event) => event.stopPropagation()} onKeyDown={(event) => { if (event.key === 'ArrowLeft') { event.preventDefault(); onAdjust(column, -10); } if (event.key === 'ArrowRight') { event.preventDefault(); onAdjust(column, 10); } }}/></span>;
}

function upsertConnectionInOrder(current: Connection[], connection: Connection): Connection[] {
  const index = current.findIndex((item) => item.id === connection.id);
  if (index < 0) return [connection, ...current];
  const next = [...current];
  next[index] = connection;
  return next;
}

export default function App() {
  const [preferences, setPreferences] = useState<Preferences>(loadPreferences);
  const language = preferences.language;
  const t = copy[language];
  const p1 = phaseOneCopy[language];
  const bookmarkLocalText = bookmarkLocalCopy[language];
  const p2 = phaseTwoCopy[language];
  const queueText = queueCopy[language];
  const columnsText = columnCopy[language];
  const viewsText = viewCopy[language];
  const syncText = syncUiCopy[language];
  const editText = remoteEditCopy[language];
  const dragText = dragOutCopy[language];
  const bookmarkOrderText = bookmarkOrderCopy[language];
  const menuText = contextMenuCopy[language];
  const browserMenuText = browserContextMenuCopy[language];
  const dropConflictText = dropConflictCopy[language];
  const [connections, setConnections] = useState<Connection[]>([]);
  const [history, setHistory] = useState<ConnectionHistory[]>([]);
  const [selectedTag, setSelectedTag] = useState('');
  const [draggedBookmarkId, setDraggedBookmarkId] = useState('');
  const [bookmarkDropTarget, setBookmarkDropTarget] = useState<{ id: string; edge: 'before' | 'after' } | null>(null);
  const [active, setActive] = useState<Connection | null>(null);
  const [path, setPath] = useState('/');
  const [pathDraft, setPathDraft] = useState('/');
  const [directoryHistory, setDirectoryHistory] = useState<string[]>([]);
  const [directoryHistoryIndex, setDirectoryHistoryIndex] = useState(-1);
  const [entries, setEntries] = useState<FileEntry[]>([]);
  const [query, setQuery] = useState('');
  const [showConnect, setShowConnect] = useState(false);
  const [showPreferences, setShowPreferences] = useState(false);
  const [showSoftwareUpdate, setShowSoftwareUpdate] = useState(false);
  const [softwareUpdate, setSoftwareUpdate] = useState<SoftwareUpdateState>({ phase: 'idle', currentVersion: '', downloadedBytes: 0 });
  const [selectedKeyPath, setSelectedKeyPath] = useState('');
  const [connectingBookmark, setConnectingBookmark] = useState<Connection | null>(null);
  const [connectSheetMode, setConnectSheetMode] = useState<'connect' | 'edit'>('connect');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [transfers, setTransfers] = useState<Transfer[]>([]);
  const [directoryProgress, setDirectoryProgress] = useState<DirectoryProgress | null>(null);
  const [directoryPaused, setDirectoryPaused] = useState(false);
  const [transferPanelCollapsed, setTransferPanelCollapsed] = useState(true);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(() => localStorage.getItem('harbor-transfer.sidebar-collapsed') === 'true');
  const [sidebarWidth, setSidebarWidth] = useState(loadSidebarWidth);
  const [columnWidths, setColumnWidths] = useState<ColumnWidths>(loadColumnWidths);
  const [columnVisibility, setColumnVisibility] = useState<ColumnVisibility>(loadColumnVisibility);
  const [sortColumn, setSortColumn] = useState<ColumnKey>('name');
  const [sortDirection, setSortDirection] = useState<SortDirection>('ascending');
  const [columnMenu, setColumnMenu] = useState<ColumnMenuPosition | null>(null);
  const [viewMode, setViewMode] = useState<ViewMode>(() => {
    const saved = localStorage.getItem('harbor-transfer.view-mode');
    return saved === 'icons' || saved === 'columns' ? saved : 'list';
  });
  const [columnLevels, setColumnLevels] = useState<ColumnLevel[]>([]);
  const [pausedTransfers, setPausedTransfers] = useState<Set<string>>(new Set());
  const [isDragOver, setIsDragOver] = useState(false);
  const [notice, setNotice] = useState('');
  const [pathCopied, setPathCopied] = useState(false);
  const [syncLocalDirectory, setSyncLocalDirectory] = useState('');
  const [syncDirection, setSyncDirection] = useState<SyncDirection>('localToRemote');
  const [syncComparison, setSyncComparison] = useState<SyncComparison>(() => (localStorage.getItem('harbor-transfer.sync-comparison') as SyncComparison) || 'sizeOnly');
  const [syncPreview, setSyncPreview] = useState<SyncPreview | null>(null);
  const [syncPreviewBusy, setSyncPreviewBusy] = useState(false);
  const [syncPreviewError, setSyncPreviewError] = useState('');
  const [syncExclusions, setSyncExclusions] = useState(() => localStorage.getItem('harbor-transfer.sync-exclusions') ?? '.DS_Store\nnode_modules/**');
  const [syncConflictChoices, setSyncConflictChoices] = useState<Record<string, SyncConflictChoice>>({});
  const [syncExecutionBusy, setSyncExecutionBusy] = useState(false);
  const [syncExecutionId, setSyncExecutionId] = useState('');
  const [syncExecutionProgress, setSyncExecutionProgress] = useState<SyncExecutionProgress | null>(null);
  const [syncExecutionResult, setSyncExecutionResult] = useState<SyncExecutionResult | null>(null);
  const [syncHistory, setSyncHistory] = useState<SyncHistory[]>([]);
  const [remoteEdits, setRemoteEdits] = useState<RemoteEdit[]>([]);
  const [selectedRemoteItems, setSelectedRemoteItems] = useState<SelectedRemoteItem[]>([]);
  const [dragExport, setDragExport] = useState<DragExport | null>(null);
  const [dragPreparingPath, setDragPreparingPath] = useState('');
  const [entryContextMenu, setEntryContextMenu] = useState<EntryContextMenu | null>(null);
  const [browserContextMenu, setBrowserContextMenu] = useState<BrowserContextMenu | null>(null);
  const [remoteClipboard, setRemoteClipboard] = useState<RemoteClipboard | null>(null);
  const [fileInformationTarget, setFileInformationTarget] = useState<FileInformationTarget | null>(null);
  const [inlineRenameTarget, setInlineRenameTarget] = useState<InlineRenameTarget | null>(null);
  const [inlineRenameBusy, setInlineRenameBusy] = useState(false);
  const [dropConflict, setDropConflict] = useState<DropConflictPrompt | null>(null);
  const availableUpdate = useRef<Update | null>(null);
  const automaticUpdateCheckStarted = useRef(false);
  const remoteEditPolling = useRef<Set<string>>(new Set());
  const dragPreparationSequence = useRef(0);
  const dragExportRef = useRef<DragExport | null>(null);
  const dragPreparingRef = useRef('');
  const bookmarkDragIdRef = useRef('');
  const bookmarkPointerDragRef = useRef<{ sourceId: string; pointerId: number; startX: number; startY: number; active: boolean } | null>(null);
  const bookmarkDropTargetRef = useRef<{ id: string; edge: 'before' | 'after' } | null>(null);
  const inlineRenameCommitting = useRef(false);
  const selectionAnchor = useRef<{ basePath: string; index: number } | null>(null);
  const transferPanelUserControlled = useRef(false);
  const browserZoneRef = useRef<HTMLElement | null>(null);
  const pathInputRef = useRef<HTMLInputElement | null>(null);
  const visibleColumns = useMemo(() => listColumnOrder.filter((column) => column === 'name' || columnVisibility[column]), [columnVisibility]);
  const filteredEntries = useMemo(() => {
    const collator = new Intl.Collator(language, { numeric: true, sensitivity: 'base' });
    const value = (entry: FileEntry, column: ColumnKey): string | number => {
      if (column === 'size') return entry.size;
      if (column === 'modified') {
        const parsed = Date.parse((entry.modified ?? '').replace(' ', 'T'));
        return Number.isNaN(parsed) ? entry.modified ?? '' : parsed;
      }
      if (column === 'permissions') return entry.permissions ?? '';
      if (column === 'owner') return entry.owner ?? '';
      if (column === 'group') return entry.group ?? '';
      if (column === 'type') return entry.file_type;
      return entry.name;
    };
    const compare = (left: FileEntry, right: FileEntry) => {
      const leftValue = value(left, sortColumn);
      const rightValue = value(right, sortColumn);
      const result = typeof leftValue === 'number' && typeof rightValue === 'number' ? leftValue - rightValue : collator.compare(String(leftValue), String(rightValue));
      const ordered = result || collator.compare(left.name, right.name);
      return sortDirection === 'ascending' ? ordered : -ordered;
    };
    const normalizedQuery = query.toLowerCase();
    return entries
      .filter((entry) => (preferences.showHiddenFiles || !entry.name.startsWith('.')) && entry.name.toLowerCase().includes(normalizedQuery))
      .sort(compare);
  }, [entries, language, preferences.showHiddenFiles, query, sortColumn, sortDirection]);
  const hasActiveTransfer = transfers.some((item) => item.status === 'Running') || Boolean(dragPreparingPath) || Boolean(directoryProgress && directoryProgress.status !== 'completed' && directoryProgress.status !== 'cancelled' && directoryProgress.status !== 'failed');
  const breadcrumbs = useMemo(() => {
    const segments = path.split('/').filter(Boolean);
    return [{ label: '/', path: '/' }, ...segments.map((segment, index) => ({ label: visiblePathComponent(segment), path: `/${segments.slice(0, index + 1).join('/')}` }))];
  }, [path]);

  async function checkForSoftwareUpdate(manual: boolean) {
    if (softwareUpdate.phase === 'checking' || softwareUpdate.phase === 'downloading') return;
    const currentVersion = softwareUpdate.currentVersion || await getVersion();
    setSoftwareUpdate({ phase: 'checking', currentVersion, downloadedBytes: 0 });
    try {
      const update = await check({ timeout: 15_000 });
      if (!update) {
        availableUpdate.current = null;
        setSoftwareUpdate({ phase: 'current', currentVersion, downloadedBytes: 0 });
        return;
      }
      availableUpdate.current = update;
      setSoftwareUpdate({ phase: 'available', currentVersion, version: update.version, body: update.body, downloadedBytes: 0 });
      setShowSoftwareUpdate(true);
    } catch (reason) {
      setSoftwareUpdate({ phase: 'error', currentVersion, downloadedBytes: 0, error: String(reason) });
      if (!manual) console.warn('Automatic software update check failed', reason);
    }
  }

  async function downloadAndInstallSoftwareUpdate() {
    const update = availableUpdate.current;
    if (!update || softwareUpdate.phase === 'downloading') return;
    let downloadedBytes = 0;
    let totalBytes: number | undefined;
    setSoftwareUpdate((current) => ({ ...current, phase: 'downloading', downloadedBytes: 0, totalBytes: undefined, error: undefined }));
    try {
      const onDownload = (event: DownloadEvent) => {
        if (event.event === 'Started') totalBytes = event.data.contentLength;
        if (event.event === 'Progress') downloadedBytes += event.data.chunkLength;
        setSoftwareUpdate((current) => ({ ...current, downloadedBytes, totalBytes }));
      };
      await update.downloadAndInstall(onDownload, { timeout: 120_000 });
      setSoftwareUpdate((current) => ({ ...current, phase: 'ready', downloadedBytes, totalBytes }));
    } catch (reason) {
      setSoftwareUpdate((current) => ({ ...current, phase: 'error', error: String(reason) }));
    }
  }

  useEffect(() => {
    void getVersion().then((currentVersion) => setSoftwareUpdate((current) => ({ ...current, currentVersion })));
    if (!preferences.autoCheckUpdates || automaticUpdateCheckStarted.current) return;
    automaticUpdateCheckStarted.current = true;
    const timeout = window.setTimeout(() => void checkForSoftwareUpdate(false), 1_500);
    return () => window.clearTimeout(timeout);
  }, [preferences.autoCheckUpdates]);

  useEffect(() => {
    void Promise.all([
      invoke<Connection[]>('bookmarks_list'),
      invoke<ConnectionHistory[]>('connection_history_list'),
      invoke<TransferHistory[]>('transfer_history_list'),
      invoke<TransferJob[]>('transfer_jobs_list'),
      invoke<SyncHistory[]>('sync_history_list'),
    ]).then(([saved, recent, transferHistory, transferJobs, savedSyncHistory]) => {
      setConnections(saved);
      setHistory(recent);
      const restored = new Map<string, Transfer>();
      transferHistory.forEach((item) => restored.set(item.id, { ...item, totalBytes: item.bytes, transferredBytes: item.bytes }));
      transferJobs.forEach((item) => restored.set(item.id, { ...item, detail: item.detail.includes('interrupted when Harbor Transfer closed') ? queueText.interrupted : item.detail }));
      setTransfers([...restored.values()]);
      setSyncHistory(savedSyncHistory);
    }).catch((reason) => setError(String(reason)));
  }, []);

  useEffect(() => {
    const closeFromPointer = (event: PointerEvent) => {
      const target = event.target as Element | null;
      if (!target?.closest('.entry-context-menu')) setEntryContextMenu(null);
      if (!target?.closest('.browser-context-menu')) setBrowserContextMenu(null);
      if (!target?.closest('.column-visibility-menu')) setColumnMenu(null);
    };
    const closeFromKey = (event: KeyboardEvent) => { if (event.key === 'Escape') { setEntryContextMenu(null); setBrowserContextMenu(null); setColumnMenu(null); } };
    const close = () => { setEntryContextMenu(null); setBrowserContextMenu(null); setColumnMenu(null); };
    document.addEventListener('pointerdown', closeFromPointer);
    document.addEventListener('keydown', closeFromKey);
    window.addEventListener('blur', close);
    window.addEventListener('resize', close);
    window.addEventListener('scroll', close, true);
    return () => {
      document.removeEventListener('pointerdown', closeFromPointer);
      document.removeEventListener('keydown', closeFromKey);
      window.removeEventListener('blur', close);
      window.removeEventListener('resize', close);
      window.removeEventListener('scroll', close, true);
    };
  }, []);

  useEffect(() => {
    if (!notice) return;
    const timeout = window.setTimeout(() => setNotice(''), 4000);
    return () => window.clearTimeout(timeout);
  }, [notice]);

  useEffect(() => {
    if (!transferPanelUserControlled.current) setTransferPanelCollapsed(!hasActiveTransfer);
  }, [hasActiveTransfer]);

  function toggleTransferPanel() {
    transferPanelUserControlled.current = true;
    setTransferPanelCollapsed((current) => !current);
  }

  useEffect(() => {
    localStorage.setItem('harbor-transfer.preferences', JSON.stringify(preferences));
    document.documentElement.dataset.theme = preferences.theme;
  }, [preferences]);

  useEffect(() => {
    if (preferences.showHiddenFiles) return;
    setSelectedRemoteItems((current) => current.filter((item) => !item.entry.name.startsWith('.')));
    selectionAnchor.current = null;
  }, [preferences.showHiddenFiles]);

  useEffect(() => {
    const title = active ? `${active.name} — ${t.title}` : t.title;
    void getCurrentWebviewWindow().setTitle(title).catch(() => undefined);
  }, [active, t.title]);

  useEffect(() => {
    const onShortcut = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey)) return;
      if (event.key.toLowerCase() === 'n') { event.preventDefault(); if (!event.repeat) void openNewWindow(); return; }
      const target = event.target as HTMLElement | null;
      if (target?.matches('input, textarea, select, [contenteditable="true"]')) return;
      if (event.key === ',') { event.preventDefault(); setShowPreferences(true); }
      if (event.key.toLowerCase() === 'r' && active) { event.preventDefault(); void loadDirectory(active, path); }
    };
    window.addEventListener('keydown', onShortcut);
    return () => window.removeEventListener('keydown', onShortcut);
  }, [active, path]);

  useEffect(() => {
    localStorage.setItem('harbor-transfer.column-widths-v2', JSON.stringify(columnWidths));
  }, [columnWidths]);

  useEffect(() => {
    localStorage.setItem('harbor-transfer.column-visibility', JSON.stringify(columnVisibility));
  }, [columnVisibility]);

  useEffect(() => {
    localStorage.setItem('harbor-transfer.view-mode', viewMode);
  }, [viewMode]);

  useEffect(() => {
    localStorage.setItem('harbor-transfer.sync-exclusions', syncExclusions);
  }, [syncExclusions]);

  useEffect(() => {
    localStorage.setItem('harbor-transfer.sync-comparison', syncComparison);
  }, [syncComparison]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<SyncExecutionProgress>('sync://progress', (event) => setSyncExecutionProgress(event.payload)).then((dispose) => { unlisten = dispose; });
    return () => unlisten?.();
  }, []);

  useEffect(() => {
    if (remoteEdits.length === 0) return;
    const interval = window.setInterval(() => {
      for (const edit of remoteEdits) {
        if (remoteEditPolling.current.has(edit.editId)) continue;
        remoteEditPolling.current.add(edit.editId);
        void invoke<RemoteEditPollResult>('remote_edit_poll', { editId: edit.editId }).then((result) => {
          const status: RemoteEdit['status'] = result.status === 'waiting' ? 'waiting' : 'watching';
          setRemoteEdits((current) => {
            let changed = false;
            const next = current.map((item) => {
              if (item.editId !== edit.editId || (item.status === status && !item.detail)) return item;
              changed = true;
              return { ...item, status, detail: undefined };
            });
            return changed ? next : current;
          });
          if (result.status === 'uploaded') {
            setNotice(`${editText.uploaded}: ${edit.name}`);
            if (active && parentPath(result.remotePath) === path) void loadDirectory(active, path);
          }
        }).catch((reason) => {
          setRemoteEdits((current) => current.map((item) => item.editId === edit.editId ? { ...item, status: 'failed', detail: String(reason) } : item));
        }).finally(() => remoteEditPolling.current.delete(edit.editId));
      }
    }, 1000);
    return () => window.clearInterval(interval);
  }, [remoteEdits, editText.uploaded, active, path]);

  useEffect(() => {
    localStorage.setItem('harbor-transfer.sidebar-collapsed', String(sidebarCollapsed));
  }, [sidebarCollapsed]);

  useEffect(() => {
    localStorage.setItem('harbor-transfer.sidebar-width', String(sidebarWidth));
  }, [sidebarWidth]);

  useEffect(() => {
    const resize = () => setSidebarWidth((current) => clampSidebarWidth(current));
    window.addEventListener('resize', resize);
    return () => window.removeEventListener('resize', resize);
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<string>('ssh-key://selected', (event) => {
      setSelectedKeyPath(event.payload);
      setConnectingBookmark(null);
      setConnectSheetMode('connect');
      setShowConnect(true);
    }).then((dispose) => { unlisten = dispose; });
    return () => unlisten?.();
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<FileProgress>('transfer://file-progress', (event) => {
      const progress = event.payload;
      const seconds = Math.max(progress.elapsedMs / 1000, 0.001);
      const speed = progress.transferredBytes / seconds;
      const remaining = Math.max(progress.totalBytes - progress.transferredBytes, 0);
      setTransfers((current) => current.map((item) => item.id === progress.transferId ? {
        ...item,
        transferredBytes: progress.transferredBytes,
        totalBytes: progress.totalBytes,
        speed,
        etaSeconds: speed > 0 ? remaining / speed : undefined,
        activity: progress.status === 'reconnecting' ? 'reconnecting' : progress.status === 'queued' ? 'queued' : 'running',
      } : item));
    }).then((dispose) => { unlisten = dispose; });
    return () => unlisten?.();
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    if (!active) return;
    void getCurrentWebview().onDragDropEvent((event) => {
      if (bookmarkDragIdRef.current) {
        setIsDragOver(false);
        return;
      }
      if (event.payload.type === 'leave') setIsDragOver(false);
      else if (event.payload.type === 'drop') {
        setIsDragOver(false);
        if (event.payload.paths.length) void uploadDroppedPaths(event.payload.paths, true);
      } else setIsDragOver(true);
    }).then((dispose) => { unlisten = dispose; }).catch(() => undefined);
    return () => unlisten?.();
  }, [active, path, entries]);

  const availableTags = useMemo(() => Array.from(new Set(connections.flatMap((connection) => connection.tags.split(',').map((tag) => tag.trim()).filter(Boolean)))).sort(), [connections]);
  const visibleConnections = useMemo(() => selectedTag ? connections.filter((connection) => connection.tags.split(',').map((tag) => tag.trim()).includes(selectedTag)) : connections, [connections, selectedTag]);

  useEffect(() => {
    setPathDraft(visibleRemotePath(path));
    const frame = window.requestAnimationFrame(() => {
      if (pathInputRef.current) pathInputRef.current.scrollLeft = 0;
    });
    return () => window.cancelAnimationFrame(frame);
  }, [path]);

  async function moveBookmark(sourceId: string, targetId: string, edge: 'before' | 'after') {
    if (selectedTag || sourceId === targetId) return;
    const previous = connections;
    const moving = previous.find((connection) => connection.id === sourceId);
    if (!moving) return;
    const next = previous.filter((connection) => connection.id !== sourceId);
    const targetIndex = next.findIndex((connection) => connection.id === targetId);
    if (targetIndex < 0) return;
    next.splice(targetIndex + (edge === 'after' ? 1 : 0), 0, moving);
    if (next.every((connection, index) => connection.id === previous[index]?.id)) return;
    setConnections(next);
    try {
      await invoke('bookmarks_reorder', { bookmarkIds: next.map((connection) => connection.id) });
      setNotice(bookmarkOrderText.saved);
    } catch (reason) {
      try { setConnections(await invoke<Connection[]>('bookmarks_list')); }
      catch { setConnections(previous); }
      setError(invokeErrorMessage(reason));
    }
  }

  function setBookmarkPointerDropTarget(target: { id: string; edge: 'before' | 'after' } | null) {
    bookmarkDropTargetRef.current = target;
    setBookmarkDropTarget((current) => current?.id === target?.id && current?.edge === target?.edge ? current : target);
  }

  function startBookmarkPointerDrag(event: React.PointerEvent<HTMLButtonElement>, id: string) {
    if (selectedTag || event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    bookmarkPointerDragRef.current = { sourceId: id, pointerId: event.pointerId, startX: event.clientX, startY: event.clientY, active: false };
    bookmarkDragIdRef.current = id;
    setIsDragOver(false);
    setBookmarkPointerDropTarget(null);
  }

  function moveBookmarkPointerDrag(event: React.PointerEvent<HTMLButtonElement>) {
    const drag = bookmarkPointerDragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    if (!drag.active) {
      if (Math.hypot(event.clientX - drag.startX, event.clientY - drag.startY) < 4) return;
      drag.active = true;
      document.body.classList.add('reordering-bookmarks');
      setDraggedBookmarkId(drag.sourceId);
    }
    event.preventDefault();
    const row = document.elementFromPoint(event.clientX, event.clientY)?.closest<HTMLElement>('[data-bookmark-id]');
    const targetId = row?.dataset.bookmarkId;
    if (!row || !targetId || targetId === drag.sourceId) {
      setBookmarkPointerDropTarget(null);
      return;
    }
    const bounds = row.getBoundingClientRect();
    const edge = event.clientY < bounds.top + bounds.height / 2 ? 'before' : 'after';
    setBookmarkPointerDropTarget({ id: targetId, edge });
  }

  function endBookmarkPointerDrag(event: React.PointerEvent<HTMLButtonElement>) {
    const drag = bookmarkPointerDragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    moveBookmarkPointerDrag(event);
    const target = bookmarkDropTargetRef.current;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
    finishBookmarkDrag();
    if (drag.active && target) void moveBookmark(drag.sourceId, target.id, target.edge);
  }

  function finishBookmarkDrag() {
    document.body.classList.remove('reordering-bookmarks');
    bookmarkPointerDragRef.current = null;
    bookmarkDragIdRef.current = '';
    bookmarkDropTargetRef.current = null;
    setIsDragOver(false);
    setDraggedBookmarkId('');
    setBookmarkDropTarget(null);
  }

  function moveBookmarkWithKeyboard(id: string, offset: -1 | 1) {
    if (selectedTag) return;
    const index = connections.findIndex((connection) => connection.id === id);
    const target = connections[index + offset];
    if (!target) return;
    void moveBookmark(id, target.id, offset < 0 ? 'before' : 'after');
  }

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<DirectoryProgress>('transfer://progress', (event) => setDirectoryProgress(event.payload)).then((dispose) => { unlisten = dispose; });
    return () => unlisten?.();
  }, []);

  async function loadDirectory(connection = active, requestedPath = path): Promise<boolean> {
    if (!connection) return false;
    setBusy(true); setError(null);
    try {
      const result = await invoke<FileEntry[]>('remote_list', { request: { connectionId: connection.id, path: requestedPath } });
      setEntries(result); setPath(requestedPath); setColumnLevels([{ path: requestedPath, entries: result }]);
      setSelectedRemoteItems([]); selectionAnchor.current = null;
      return true;
    } catch (reason) { setError(String(reason)); return false; }
    finally { setBusy(false); }
  }

  function recordDirectoryNavigation(nextPath: string, reset = false) {
    if (reset) {
      setDirectoryHistory([nextPath]);
      setDirectoryHistoryIndex(0);
      return;
    }
    if (directoryHistory[directoryHistoryIndex] === nextPath) return;
    const nextHistory = [...directoryHistory.slice(0, directoryHistoryIndex + 1), nextPath];
    setDirectoryHistory(nextHistory);
    setDirectoryHistoryIndex(nextHistory.length - 1);
  }

  async function navigateDirectory(connection = active, requestedPath = path, resetHistory = false) {
    if (resetHistory) {
      setDirectoryHistory([]);
      setDirectoryHistoryIndex(-1);
    }
    if (await loadDirectory(connection, requestedPath)) recordDirectoryNavigation(requestedPath, resetHistory);
  }

  async function navigateDirectoryHistory(offset: -1 | 1) {
    const nextIndex = directoryHistoryIndex + offset;
    const requestedPath = directoryHistory[nextIndex];
    if (!active || requestedPath === undefined) return;
    if (await loadDirectory(active, requestedPath)) setDirectoryHistoryIndex(nextIndex);
  }

  function selectViewMode(next: ViewMode) {
    setViewMode(next);
    if (next === 'columns') setColumnLevels([{ path, entries }]);
  }

  async function openColumnEntry(levelIndex: number, entry: FileEntry) {
    if (!active) return;
    const level = columnLevels[levelIndex];
    if (!level) return;
    const selectedLevels = columnLevels.slice(0, levelIndex + 1).map((item, index) => index === levelIndex ? { ...item, selectedName: entryIdentity(entry) } : item);
    if (entry.file_type !== 'Directory') {
      setColumnLevels(selectedLevels);
      setPath(level.path);
      setEntries(level.entries);
      return;
    }
    const childPath = entryRemotePath(level.path, entry);
    setBusy(true); setError(null); setColumnLevels(selectedLevels);
    try {
      const childEntries = await invoke<FileEntry[]>('remote_list', { request: { connectionId: active.id, path: childPath } });
      setColumnLevels([...selectedLevels, { path: childPath, entries: childEntries }]);
      setPath(childPath);
      setEntries(childEntries);
      recordDirectoryNavigation(childPath);
    } catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  }

  function adjustColumnWidth(column: ColumnKey, delta: number) {
    setColumnWidths((current) => ({ ...current, [column]: Math.min(maximumColumnWidths[column], Math.max(minimumColumnWidths[column], current[column] + delta)) }));
  }

  function adjustSidebarWidth(delta: number) {
    setSidebarWidth((current) => clampSidebarWidth(current + delta));
  }

  function startSidebarResize(event: React.PointerEvent<HTMLDivElement>) {
    event.preventDefault();
    event.stopPropagation();
    const startX = event.clientX;
    const startWidth = sidebarWidth;
    document.body.classList.add('resizing-sidebar');
    const move = (moveEvent: PointerEvent) => setSidebarWidth(clampSidebarWidth(startWidth + moveEvent.clientX - startX));
    const stop = () => {
      document.body.classList.remove('resizing-sidebar');
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', stop);
      window.removeEventListener('pointercancel', stop);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', stop);
    window.addEventListener('pointercancel', stop);
  }

  function startColumnResize(event: React.PointerEvent, column: ColumnKey) {
    event.preventDefault();
    event.stopPropagation();
    const startX = event.clientX;
    const startWidth = columnWidths[column];
    document.body.classList.add('resizing-columns');
    const move = (moveEvent: PointerEvent) => setColumnWidths((current) => ({ ...current, [column]: Math.min(maximumColumnWidths[column], Math.max(minimumColumnWidths[column], startWidth + moveEvent.clientX - startX)) }));
    const stop = () => {
      document.body.classList.remove('resizing-columns');
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', stop);
      window.removeEventListener('pointercancel', stop);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', stop);
    window.addEventListener('pointercancel', stop);
  }

  function sortByColumn(column: ColumnKey) {
    if (sortColumn === column) setSortDirection((current) => current === 'ascending' ? 'descending' : 'ascending');
    else { setSortColumn(column); setSortDirection('ascending'); }
  }

  function openColumnVisibilityMenu(event: React.MouseEvent) {
    event.preventDefault();
    event.stopPropagation();
    setEntryContextMenu(null);
    setBrowserContextMenu(null);
    setColumnMenu({ x: Math.max(8, Math.min(event.clientX, window.innerWidth - 220)), y: Math.max(8, Math.min(event.clientY, window.innerHeight - 300)) });
  }

  function toggleColumn(column: OptionalColumnKey) {
    setColumnVisibility((current) => ({ ...current, [column]: !current[column] }));
    if (sortColumn === column && columnVisibility[column]) { setSortColumn('name'); setSortDirection('ascending'); }
    setColumnMenu(null);
  }

  function columnLabel(column: ColumnKey) {
    if (column === 'name') return t.name;
    if (column === 'size') return t.size;
    if (column === 'modified') return t.modified;
    return columnsText[column];
  }

  function entryKind(entry: FileEntry) {
    if (entry.file_type === 'Directory') return columnsText.directory;
    if (entry.file_type === 'Symlink') return columnsText.symlink;
    return columnsText.file;
  }

  function isInlineRenaming(entry: FileEntry, basePath: string) {
    const target = inlineRenameTarget;
    return Boolean(target && target.connectionId === active?.id && target.remotePath === entryRemotePath(basePath, entry));
  }

  function beginInlineRename(event: React.SyntheticEvent, entry: FileEntry, basePath: string) {
    if (!active) return;
    event.preventDefault();
    event.stopPropagation();
    cancelScheduledDragPreparation();
    const item = selectedItem(entry, basePath);
    if (!item) return;
    setEntryContextMenu(null);
    setBrowserContextMenu(null);
    setSelectedRemoteItems([item]);
    setInlineRenameTarget({ ...item, value: entry.name });
  }

  function cancelInlineRename() {
    if (inlineRenameCommitting.current) return;
    setInlineRenameTarget(null);
  }

  function applyInlineRenameResult(target: SelectedRemoteItem, name: string, refreshed?: FileEntry[]) {
    const destinationPath = joinPath(target.basePath, name);
    const renamedEntry = refreshed?.find((entry) => entry.name === name) ?? { ...target.entry, name };
    const replaceRenamedEntry = (current: FileEntry[]) => refreshed ?? current.map((entry) => entryIdentity(entry) === entryIdentity(target.entry) ? renamedEntry : entry);

    if (path === target.basePath || (target.entry.file_type === 'Directory' && path.startsWith(`${target.remotePath}/`))) {
      setEntries(replaceRenamedEntry);
    }
    setColumnLevels((current) => {
      const levelIndex = current.findIndex((level) => level.path === target.basePath);
      if (levelIndex < 0) return current;
      const next = current.slice(0, target.entry.file_type === 'Directory' ? levelIndex + 1 : current.length);
      next[levelIndex] = { ...next[levelIndex], entries: replaceRenamedEntry(next[levelIndex].entries), selectedName: entryIdentity(renamedEntry) };
      return next;
    });
    if (target.entry.file_type === 'Directory' && path !== target.basePath && path.startsWith(`${target.remotePath}/`)) {
      setPath(target.basePath);
    }
    setSelectedRemoteItems([{ connectionId: target.connectionId, remotePath: destinationPath, basePath: target.basePath, entry: renamedEntry }]);
  }

  async function commitInlineRename() {
    if (!active || !inlineRenameTarget || inlineRenameCommitting.current) return;
    const target = inlineRenameTarget;
    const name = target.value.trim();
    if (!name || name.includes('/') || name === '.' || name === '..') {
      setError(menuText.invalidName);
      return;
    }
    if (name === target.entry.name) { setInlineRenameTarget(null); return; }
    inlineRenameCommitting.current = true;
    setInlineRenameBusy(true);
    setError(null);
    try {
      const destinationPath = joinPath(target.basePath, name);
      await invoke('remote_rename', { request: { connectionId: active.id, oldPath: target.remotePath, newPath: destinationPath } });
      applyInlineRenameResult(target, name);
      setInlineRenameTarget(null);
      setNotice(menuText.saved);
      void invoke<FileEntry[]>('remote_list', { request: { connectionId: active.id, path: target.basePath } })
        .then((refreshed) => {
          if (active.id === target.connectionId) applyInlineRenameResult(target, name, refreshed);
        })
        .catch(() => undefined);
    } catch (reason) {
      const message = invokeErrorMessage(reason);
      if (/timeout/i.test(message)) {
        try {
          const refreshed = await invoke<FileEntry[]>('remote_list', { request: { connectionId: active.id, path: target.basePath } });
          const renameCompleted = refreshed.some((entry) => entry.name === name)
            && !refreshed.some((entry) => entry.name === target.entry.name);
          if (renameCompleted) {
            applyInlineRenameResult(target, name, refreshed);
            setInlineRenameTarget(null);
            setNotice(menuText.saved);
            return;
          }
        } catch {
          // Preserve the original rename error when the server cannot confirm its result.
        }
      }
      setError(message);
    }
    finally {
      inlineRenameCommitting.current = false;
      setInlineRenameBusy(false);
    }
  }

  function inlineRenameInput(mode: ViewMode) {
    if (!inlineRenameTarget) return null;
    return <input
      className={`inline-rename-input ${mode}`}
      aria-label={menuText.rename}
      value={inlineRenameTarget.value}
      readOnly={inlineRenameBusy}
      autoFocus
      {...technicalInputProps}
      onFocus={(event) => event.currentTarget.select()}
      onChange={(event) => setInlineRenameTarget((current) => current ? { ...current, value: event.target.value } : current)}
      onClick={(event) => event.stopPropagation()}
      onDoubleClick={(event) => event.stopPropagation()}
      onPointerDown={(event) => event.stopPropagation()}
      onBlur={cancelInlineRename}
      onKeyDown={(event) => {
        event.stopPropagation();
        if (event.key === 'Enter') { event.preventDefault(); void commitInlineRename(); }
        if (event.key === 'Escape') { event.preventDefault(); setInlineRenameTarget(null); }
      }}
    />;
  }

  function renderListCell(entry: FileEntry, column: ColumnKey, basePath: string, selected: boolean) {
    if (column === 'name') return <span className="file-name"><span className="file-name-icon"><RemoteEntryIcon entry={entry} size={18}/></span>{isInlineRenaming(entry, basePath) ? inlineRenameInput('list') : <span className="file-name-text" title={selected ? menuText.rename : entry.name} onClick={(event) => { if (selected) beginInlineRename(event, entry, basePath); }}>{entry.name}</span>}</span>;
    if (column === 'size') return <span className="size-cell">{entry.file_type === 'Directory' ? '—' : formatBytes(entry.size)}</span>;
    if (column === 'modified') return <span className="modified-cell">{entry.modified ?? '—'}</span>;
    if (column === 'permissions') return <span className="permissions-cell">{entry.permissions ?? '—'}</span>;
    if (column === 'owner') return <span className="owner-cell">{entry.owner ?? '—'}</span>;
    if (column === 'group') return <span className="group-cell">{entry.group ?? '—'}</span>;
    return <span className="type-cell">{entryKind(entry)}</span>;
  }

  async function copyCurrentPath() {
    try {
      if (navigator.clipboard?.writeText) await navigator.clipboard.writeText(visibleRemotePath(path));
      else {
        const temporary = document.createElement('textarea');
        temporary.value = path;
        temporary.style.position = 'fixed';
        temporary.style.opacity = '0';
        document.body.appendChild(temporary);
        temporary.select();
        document.execCommand('copy');
        temporary.remove();
      }
      setPathCopied(true);
      setNotice(t.pathCopied);
      window.setTimeout(() => setPathCopied(false), 1800);
    } catch (reason) { setError(String(reason)); }
  }

  async function openKeyManagerWindow() {
    try {
      const sourceLabel = getCurrentWebviewWindow().label;
      const keyWindowLabel = `ssh-key-manager-${sourceLabel}`;
      const existing = await WebviewWindow.getByLabel(keyWindowLabel);
      if (existing) {
        await existing.show();
        await existing.setFocus();
        return;
      }
      const keyWindow = new WebviewWindow(keyWindowLabel, {
        url: `index.html?target=${encodeURIComponent(sourceLabel)}#ssh-keys`,
        title: t.keyManager,
        width: 720,
        height: 560,
        minWidth: 520,
        minHeight: 360,
        center: true,
        resizable: true,
        focus: true,
      });
      await keyWindow.once('tauri://error', (event) => setError(String(event.payload)));
    } catch (reason) { setError(String(reason)); }
  }

  async function openNewWindow() {
    try {
      const label = `main-${crypto.randomUUID()}`;
      const appWindow = new WebviewWindow(label, {
        url: 'index.html',
        title: t.title,
        width: 1240,
        height: 780,
        minWidth: 900,
        minHeight: 620,
        center: true,
        resizable: true,
        focus: true,
        titleBarStyle: 'overlay',
        hiddenTitle: true,
        trafficLightPosition: new LogicalPosition(14, 19),
      });
      await appWindow.once('tauri://error', (event) => setError(`${windowCopy[language].newWindowFailed}: ${String(event.payload)}`));
    } catch (reason) { setError(`${windowCopy[language].newWindowFailed}: ${invokeErrorMessage(reason)}`); }
  }

  async function exportBookmarks() {
    if (connections.length === 0) { setNotice(t.noBookmarksToExport); return; }
    setError(null);
    try {
      const date = new Date().toISOString().slice(0, 10);
      const selected = await save({
        defaultPath: `Harbor-Transfer-Bookmarks-${date}.json`,
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!selected) return;
      const payload: BookmarkExportFile = {
        format: 'harbor-transfer-bookmarks',
        version: 1,
        exportedAt: new Date().toISOString(),
        bookmarks: connections,
      };
      await writeTextFile(selected, `${JSON.stringify(payload, null, 2)}\n`);
      setNotice(t.bookmarksExported);
    } catch (reason) { setError(String(reason)); }
  }

  async function importBookmarks() {
    setError(null);
    try {
      const selected = await open({ multiple: false, directory: false, filters: [{ name: 'JSON', extensions: ['json'] }] });
      if (!selected || Array.isArray(selected)) return;
      const imported = parseBookmarkExport(await readTextFile(selected));
      if (!imported) { setError(t.invalidBookmarkFile); return; }
      const prepared = imported.map((bookmark) => {
        const existing = connections.find((connection) => connection.id === bookmark.id);
        const targetChanged = existing && connectionTargetChanged(existing, bookmark);
        return targetChanged ? { ...bookmark, id: crypto.randomUUID() } : bookmark;
      });
      for (const bookmark of prepared) await invoke('bookmark_save', { bookmark });
      const importedIds = new Set(prepared.map((bookmark) => bookmark.id));
      const importedFirst = await invoke<Connection[]>('bookmarks_list');
      const bookmarkIds = [
        ...prepared.map((bookmark) => bookmark.id),
        ...importedFirst.filter((bookmark) => !importedIds.has(bookmark.id)).map((bookmark) => bookmark.id),
      ];
      await invoke('bookmarks_reorder', { bookmarkIds });
      const saved = await invoke<Connection[]>('bookmarks_list');
      setConnections(saved);
      setSelectedTag('');
      setActive((current) => {
        if (!current) return current;
        const updated = saved.find((bookmark) => bookmark.id === current.id);
        if (!updated) return current;
        const targetChanged = connectionTargetChanged(current, updated);
        return targetChanged ? null : { ...current, name: updated.name, initialPath: updated.initialPath, keyPath: updated.keyPath, keyPassphraseNotRequired: updated.keyPassphraseNotRequired, localDirectory: updated.localDirectory, tags: updated.tags, transferMaxConcurrent: updated.transferMaxConcurrent, transferBandwidthLimitKbps: updated.transferBandwidthLimitKbps, transferRetryCount: updated.transferRetryCount };
      });
      setNotice(t.bookmarksImported.replace('{{count}}', String(imported.length)));
    } catch (reason) { setError(String(reason)); }
  }

  function startWindowDrag(event: React.PointerEvent<HTMLElement>) {
    if (event.button !== 0) return;
    const target = event.target as HTMLElement;
    if (target.closest('button, input, select, label, [data-no-window-drag]')) return;
    event.preventDefault();
    void getCurrentWebviewWindow().startDragging().catch((reason) => setError(String(reason)));
  }

  function resolveRemoteName(name: string): string | null {
    if (!entries.some((entry) => entry.name === name)) return name;
    if (preferences.conflictPolicy === 'overwrite') return name;
    if (preferences.conflictPolicy === 'skip') return null;
    const action = window.prompt(`${p2.conflict}\n${p2.overwrite} / ${p2.skip} / ${p2.rename}`, p2.skip)?.toLowerCase();
    if (!action || action === p2.skip.toLowerCase() || action === 'skip') return null;
    if (action === p2.rename.toLowerCase() || action === 'rename') return window.prompt(p2.renameTo, `copy-${name}`);
    return name;
  }

  function recordTransfer(transfer: Transfer, status: 'Completed' | 'Failed' | 'Cancelled', detail = transfer.detail, bytes = transfer.totalBytes ?? transfer.transferredBytes ?? 0) {
    if (preferences.transferNotifications) setNotice(`${status === 'Completed' ? p2.completed : status === 'Cancelled' ? p2.cancelled : p2.failed}: ${transfer.name}`);
    void invoke('transfer_history_record', {
      transfer: { id: transfer.id, name: transfer.name, direction: transfer.direction, status, detail, bytes, completedAt: '' },
    }).catch((reason) => setError(String(reason)));
  }

  function verifiedTransferDetail(detail: string, outcome: TransferOutcome) {
    const labels = {
      ja: { sha256: 'SHA-256検証済み', size: '転送サイズ検証済み' },
      en: { sha256: 'SHA-256 verified', size: 'Transfer size verified' },
      'zh-CN': { sha256: 'SHA-256 已验证', size: '传输大小已验证' },
    } as const;
    const verification = labels[language][outcome.verification];
    return detail ? `${detail} · ${verification}` : verification;
  }

  async function enqueueFile(localPath: string, name: string, resolvedName?: string): Promise<boolean> {
    if (!active) return false;
    const remoteName = resolvedName ?? resolveRemoteName(name);
    if (!remoteName) return false;
    const transferId = crypto.randomUUID();
    const transfer: Transfer = { id: transferId, name: remoteName, direction: 'Upload', status: 'Running', detail: path, localPath, remotePath: joinPath(path, remoteName), connectionId: active.id, transferredBytes: 0 };
    setTransfers((current) => [transfer, ...current]);
    try {
      const outcome = await invoke<TransferOutcome>('transfer_upload', { request: { transferId, connectionId: active.id, localPath, remotePath: transfer.remotePath, name: transfer.name, conflictPolicy: preferences.conflictPolicy, ...transferLimits(preferences, active) } });
      setTransfers((current) => current.map((item) => item.id === transfer.id ? { ...item, status: 'Completed' } : item));
      recordTransfer(transfer, 'Completed', verifiedTransferDetail(transfer.detail, outcome), outcome.bytes);
      return true;
    } catch (reason) {
      const status = transferFailureStatus(reason);
      setTransfers((current) => current.map((item) => item.id === transfer.id ? { ...item, status, detail: String(reason) } : item));
      recordTransfer(transfer, status, String(reason));
      return false;
    }
  }

  async function enqueueDirectory(localDirectory: string, name: string, resolvedName?: string): Promise<boolean> {
    if (!active) return false;
    const remoteName = resolvedName ?? resolveRemoteName(name);
    if (!remoteName) return false;
    const transferId = crypto.randomUUID();
    const transfer: Transfer = { id: transferId, name: remoteName, direction: 'Upload', status: 'Running', detail: path, localPath: localDirectory, remotePath: joinPath(path, remoteName), connectionId: active.id, isDirectory: true, conflictPolicy: preferences.conflictPolicy };
    setDirectoryProgress({ transferId, completedFiles: 0, totalFiles: 0, currentPath: remoteName, status: 'preparing' });
    setDirectoryPaused(false);
    setTransfers((current) => [transfer, ...current]);
    try {
      await invoke('transfer_upload_directory', { request: { transferId, connectionId: active.id, localDirectory, remoteDirectory: transfer.remotePath, name: transfer.name, conflictPolicy: preferences.conflictPolicy, ...transferLimits(preferences, active) } });
      setTransfers((current) => current.map((item) => item.id === transfer.id ? { ...item, status: 'Completed' } : item));
      recordTransfer(transfer, 'Completed');
      void loadDirectory();
      return true;
    } catch (reason) {
      const status = transferFailureStatus(reason);
      setTransfers((current) => current.map((item) => item.id === transfer.id ? { ...item, status, detail: String(reason) } : item));
      recordTransfer(transfer, status, String(reason));
      return false;
    }
  }

  function askDropConflict(name: string, incomingIsDirectory: boolean, existingIsDirectory: boolean): Promise<DropConflictChoice> {
    return new Promise((resolve) => setDropConflict({ name, incomingIsDirectory, existingIsDirectory, resolve }));
  }

  function settleDropConflict(choice: DropConflictChoice) {
    const prompt = dropConflict;
    setDropConflict(null);
    prompt?.resolve(choice);
  }

  async function uploadDroppedPaths(paths: string[], dropped = false) {
    const knownRemoteEntries = new Map(entries.map((entry) => [entry.name, entry]));
    for (const localPath of paths) {
      try {
        const info = await invoke<LocalPathInfo>('local_path_info', { path: localPath });
        if (dropped) {
          const existing = knownRemoteEntries.get(info.name);
          if (existing) {
            const existingIsDirectory = existing.file_type === 'Directory';
            const choice = await askDropConflict(info.name, info.isDirectory, existingIsDirectory);
            if (choice === 'cancel') continue;
            const mustDeleteExisting = choice === 'replace'
              || (choice === 'overwrite' && (info.isDirectory !== existingIsDirectory || existing.file_type === 'Symlink'));
            if (mustDeleteExisting && active) {
              await invoke('remote_delete_tree', { request: { connectionId: active.id, path: joinPath(path, info.name), isDirectory: existingIsDirectory } });
            }
          }
          const uploaded = info.isDirectory
            ? await enqueueDirectory(localPath, info.name, info.name)
            : await enqueueFile(localPath, info.name, info.name);
          if (uploaded) knownRemoteEntries.set(info.name, { name: info.name, size: 0, file_type: info.isDirectory ? 'Directory' : 'File' });
        } else if (info.isDirectory) await enqueueDirectory(localPath, info.name);
        else await enqueueFile(localPath, info.name);
      } catch (reason) { setError(String(reason)); }
    }
    void loadDirectory();
  }

  async function uploadFiles() {
    const selected = await open({ multiple: true, directory: false });
    await uploadDroppedPaths(Array.isArray(selected) ? selected : selected ? [selected] : []);
  }

  async function uploadDirectory() {
    const selected = await open({ multiple: false, directory: true });
    if (selected && !Array.isArray(selected)) await uploadDroppedPaths([selected]);
  }

  function parsedSyncExclusions(value = syncExclusions) {
    return value.split(/\r?\n/).map((pattern) => pattern.trim()).filter(Boolean);
  }

  async function calculateSyncPreview(localDirectory = syncLocalDirectory, direction = syncDirection, exclusions = syncExclusions, comparison = syncComparison) {
    if (!active || !localDirectory) return;
    setSyncPreviewBusy(true); setSyncPreviewError('');
    try {
      const preview = await invoke<SyncPreview>('sync_preview', { request: { connectionId: active.id, localDirectory, remoteDirectory: path, direction, exclusions: parsedSyncExclusions(exclusions), comparison } });
      setSyncPreview(preview);
      setSyncConflictChoices(Object.fromEntries(preview.items.filter((item) => item.action === 'conflict').map((item) => [item.path, 'skip'])));
    } catch (reason) { setSyncPreviewError(String(reason)); }
    finally { setSyncPreviewBusy(false); }
  }

  async function openSyncPreview() {
    const selected = active?.localDirectory || await open({ multiple: false, directory: true });
    if (!selected || Array.isArray(selected)) return;
    setSyncLocalDirectory(selected);
    setSyncDirection('localToRemote');
    setSyncPreview(null);
    setSyncExecutionResult(null);
    await calculateSyncPreview(selected, 'localToRemote', syncExclusions);
  }

  async function executeSync() {
    if (!active || !syncPreview || syncExecutionBusy) return;
    const items = syncPreview.items.flatMap((item) => {
      if (item.action === 'destinationOnly') return [];
      if (item.action !== 'conflict') return [{ path: item.path, action: item.action }];
      if (item.isDirectory || syncConflictChoices[item.path] !== 'source') return [];
      return [{ path: item.path, action: syncDirection === 'localToRemote' ? 'upload' : 'download' }];
    });
    if (items.length === 0 || !window.confirm(syncText.confirmExecute.replace('{{count}}', String(items.length)))) return;
    const syncId = crypto.randomUUID();
    setSyncExecutionId(syncId); setSyncExecutionBusy(true); setSyncExecutionResult(null); setSyncPreviewError('');
    setSyncExecutionProgress({ syncId, completedItems: 0, totalItems: items.length, currentPath: '', status: 'Running' });
    try {
      const result = await invoke<SyncExecutionResult>('sync_execute', { request: { syncId, connectionId: active.id, localDirectory: syncLocalDirectory, remoteDirectory: path, direction: syncDirection, exclusions: parsedSyncExclusions(), comparison: syncComparison, items } });
      setSyncExecutionResult(result);
      setSyncHistory(await invoke<SyncHistory[]>('sync_history_list'));
      await loadDirectory(active, path);
      await calculateSyncPreview(syncLocalDirectory, syncDirection, syncExclusions);
    } catch (reason) { setSyncPreviewError(String(reason)); }
    finally { setSyncExecutionBusy(false); setSyncExecutionId(''); }
  }

  async function cancelSyncExecution() {
    if (!syncExecutionId) return;
    try { await invoke('transfer_cancel', { transferId: syncExecutionId }); }
    catch (reason) { setSyncPreviewError(String(reason)); }
  }

  async function clearSyncHistory() {
    if (!window.confirm(`${syncText.clearHistory}?`)) return;
    try { await invoke('sync_history_clear'); setSyncHistory([]); }
    catch (reason) { setSyncPreviewError(String(reason)); }
  }

  async function controlDirectoryTransfer(action: 'pause' | 'resume' | 'cancel') {
    if (!directoryProgress) return;
    await invoke(`transfer_${action}`, { transferId: directoryProgress.transferId });
    if (action === 'pause') setDirectoryPaused(true);
    if (action === 'resume') setDirectoryPaused(false);
  }

  async function controlTransfer(transfer: Transfer, action: 'pause' | 'resume' | 'cancel') {
    await invoke(`transfer_${action}`, { transferId: transfer.id });
    setPausedTransfers((current) => {
      const next = new Set(current);
      if (action === 'pause') next.add(transfer.id);
      else next.delete(transfer.id);
      return next;
    });
    if (action === 'cancel') {
      setTransfers((current) => current.map((item) => item.id === transfer.id ? { ...item, status: 'Cancelled' } : item));
      recordTransfer(transfer, 'Cancelled');
    }
  }

  async function clearConnectionHistory() {
    if (!window.confirm(queueText.confirmConnection)) return;
    try { await invoke('connection_history_clear'); setHistory([]); }
    catch (reason) { setError(String(reason)); }
  }

  async function clearTransferHistory() {
    if (!window.confirm(queueText.confirmTransfer)) return;
    try {
      await invoke('transfer_history_clear');
      await invoke('transfer_jobs_clear');
      setTransfers((current) => current.filter((item) => item.status === 'Running'));
    } catch (reason) { setError(String(reason)); }
  }

  async function retryTransfer(transfer: Transfer) {
    if (!transfer.connectionId || !transfer.localPath || !transfer.remotePath) return;
    const transferConnection = connections.find((item) => item.id === transfer.connectionId);
    if (!transferConnection) return;
    setTransfers((current) => current.map((item) => item.id === transfer.id ? { ...item, status: 'Running' } : item));
    try {
      if (transfer.isDirectory) {
        await invoke('transfer_upload_directory', { request: { transferId: transfer.id, connectionId: transfer.connectionId, localDirectory: transfer.localPath, remoteDirectory: transfer.remotePath, name: transfer.name, conflictPolicy: transfer.conflictPolicy ?? preferences.conflictPolicy, ...transferLimits(preferences, transferConnection) } });
        setTransfers((current) => current.map((item) => item.id === transfer.id ? { ...item, status: 'Completed' } : item));
        recordTransfer(transfer, 'Completed');
        return;
      }
      const outcome = await invoke<TransferOutcome>(transfer.direction === 'Upload' ? 'transfer_upload' : 'transfer_download', { request: { transferId: transfer.id, connectionId: transfer.connectionId, localPath: transfer.localPath, remotePath: transfer.remotePath, name: transfer.name, conflictPolicy: transfer.conflictPolicy ?? preferences.conflictPolicy, resumeFrom: transfer.transferredBytes ?? 0, ...transferLimits(preferences, transferConnection) } });
      setTransfers((current) => current.map((item) => item.id === transfer.id ? { ...item, status: 'Completed' } : item));
      recordTransfer(transfer, 'Completed', verifiedTransferDetail(transfer.detail, outcome), outcome.bytes);
    } catch (reason) { const status = transferFailureStatus(reason); setTransfers((current) => current.map((item) => item.id === transfer.id ? { ...item, status, detail: String(reason) } : item)); recordTransfer(transfer, status, String(reason)); }
  }

  async function downloadFile(entry: FileEntry, basePath = path) {
    if (!active || entry.file_type === 'Directory') return;
    const localPath = await save({ defaultPath: localDownloadName(entry) });
    if (!localPath) return;
    const transfer: Transfer = { id: crypto.randomUUID(), name: entry.name, direction: 'Download', status: 'Running', detail: visibleRemotePath(basePath), localPath, remotePath: entryRemotePath(basePath, entry), connectionId: active.id, transferredBytes: 0, totalBytes: entry.size };
    setTransfers((current) => [transfer, ...current]);
    try {
      const outcome = await invoke<TransferOutcome>('transfer_download', { request: { transferId: transfer.id, connectionId: active.id, localPath, remotePath: transfer.remotePath, name: transfer.name, conflictPolicy: preferences.conflictPolicy, ...transferLimits(preferences, active) } });
      setTransfers((current) => current.map((item) => item.id === transfer.id ? { ...item, status: 'Completed' } : item));
      recordTransfer(transfer, 'Completed', verifiedTransferDetail(transfer.detail, outcome), outcome.bytes);
    } catch (reason) { const status = transferFailureStatus(reason); setTransfers((current) => current.map((item) => item.id === transfer.id ? { ...item, status, detail: String(reason) } : item)); recordTransfer(transfer, status, String(reason)); }
  }

  async function createDirectory(basePath = path) {
    if (!active) return;
    const name = window.prompt(t.newFolder);
    if (!name) return;
    try {
      await invoke('remote_create_directory', { request: { connectionId: active.id, path: joinPath(basePath, name) } });
      if (basePath === path) await loadDirectory();
      else {
        const refreshed = await invoke<FileEntry[]>('remote_list', { request: { connectionId: active.id, path: basePath } });
        setColumnLevels((current) => current.map((level) => level.path === basePath ? { ...level, entries: refreshed } : level));
      }
    }
    catch (reason) { setError(String(reason)); }
  }

  async function editRemoteFile(entry: FileEntry, basePath = path) {
    if (!active || entry.file_type !== 'File') return;
    if (entry.download_name) {
      await downloadFile(entry, basePath);
      return;
    }
    const editorPath = preferences.editorPath.trim();
    if (!editorPath) {
      // A double-click must never turn into a native file/download dialog.
      // Editor selection belongs to Preferences, where it can be reviewed and
      // saved explicitly before remote files are opened.
      setError(editText.configure);
      return;
    }
    const remotePath = entryRemotePath(basePath, entry);
    const existing = remoteEdits.find((edit) => edit.connectionId === active.id && edit.remotePath === remotePath);
    if (existing) {
      setNotice(`${editText.opening}: ${entry.name}`);
      try {
        await invoke('remote_edit_reopen', { editId: existing.editId, editorPath });
        setNotice(`${editText.watching}: ${entry.name}`);
      } catch (reason) { setError(String(reason)); }
      return;
    }
    setNotice(`${editText.opening}: ${entry.name}`);
    try {
      const session = await invoke<RemoteEditOpenResult>('remote_edit_open', { request: { connectionId: active.id, remotePath, editorPath } });
      setRemoteEdits((current) => [{ ...session, connectionId: active.id, status: 'watching' }, ...current]);
      setNotice(`${editText.watching}: ${entry.name}`);
    } catch (reason) { setError(String(reason)); }
  }

  async function cleanupDragExport(target = dragExportRef.current, delayMs = 0) {
    if (!target) return;
    if (dragExportRef.current?.exportId === target.exportId) {
      dragExportRef.current = null;
      setDragExport(null);
    }
    try { await invoke('drag_export_cleanup', { exportId: target.exportId, delayMs }); }
    catch (reason) { setError(String(reason)); }
  }

  function selectedItem(entry: FileEntry, basePath: string): SelectedRemoteItem | null {
    if (!active) return null;
    return { connectionId: active.id, remotePath: entryRemotePath(basePath, entry), basePath, entry };
  }

  function selectRemoteEntry(event: { metaKey: boolean; ctrlKey: boolean; shiftKey: boolean }, entry: FileEntry, basePath: string, orderedEntries: FileEntry[]) {
    const item = selectedItem(entry, basePath);
    if (!item) return;
    const index = orderedEntries.findIndex((candidate) => entryIdentity(candidate) === entryIdentity(entry));
    if (event.shiftKey && selectionAnchor.current?.basePath === basePath && index >= 0) {
      const start = Math.min(selectionAnchor.current.index, index);
      const end = Math.max(selectionAnchor.current.index, index);
      const range = orderedEntries.slice(start, end + 1).map((candidate) => selectedItem(candidate, basePath)).filter((candidate): candidate is SelectedRemoteItem => candidate !== null);
      setSelectedRemoteItems((current) => event.metaKey || event.ctrlKey ? [...current.filter((currentItem) => !range.some((rangeItem) => rangeItem.remotePath === currentItem.remotePath)), ...range] : range);
      return;
    }
    selectionAnchor.current = { basePath, index: Math.max(index, 0) };
    if (event.metaKey || event.ctrlKey) {
      setSelectedRemoteItems((current) => current.some((currentItem) => currentItem.connectionId === item.connectionId && currentItem.remotePath === item.remotePath) ? current.filter((currentItem) => currentItem.connectionId !== item.connectionId || currentItem.remotePath !== item.remotePath) : [...current, item]);
    } else setSelectedRemoteItems([item]);
  }

  function scheduleRemoteDragPreparation(event: { metaKey: boolean; ctrlKey: boolean; shiftKey: boolean }, entry: FileEntry, basePath = path, orderedEntries = filteredEntries) {
    if (!active) return;
    selectRemoteEntry(event, entry, basePath, orderedEntries);
  }

  function cancelScheduledDragPreparation() {
    // Export preparation now starts only from an actual drag gesture. Invalidate
    // any preparation already running when another action takes ownership.
    if (!dragPreparingRef.current) return;
    dragPreparationSequence.current += 1;
    dragPreparingRef.current = '';
    setDragPreparingPath('');
  }

  async function prepareRemoteDrag(entry: FileEntry, basePath = path) {
    if (!active) return;
    const remotePath = entryRemotePath(basePath, entry);
    const item = selectedItem(entry, basePath);
    if (item) setSelectedRemoteItems([item]);
    if (entry.file_type === 'Symlink') {
      dragPreparationSequence.current += 1;
      dragPreparingRef.current = '';
      setDragPreparingPath('');
      await cleanupDragExport();
      return;
    }
    const prepared = dragExportRef.current;
    if (prepared?.connectionId === active.id && prepared.remotePath === remotePath) return;
    if (dragPreparingRef.current === remotePath) return;
    const sequence = ++dragPreparationSequence.current;
    dragPreparingRef.current = remotePath;
    setDragPreparingPath(remotePath);
    setNotice(`${dragText.preparing}: ${entry.name}`);
    await cleanupDragExport(prepared);
    try {
      const result = await invoke<Omit<DragExport, 'connectionId'>>('drag_export_prepare', { request: { connectionId: active.id, remotePath, isDirectory: entry.file_type === 'Directory', displayName: localDownloadName(entry) } });
      const next = { ...result, connectionId: active.id };
      if (sequence !== dragPreparationSequence.current) {
        await invoke('drag_export_cleanup', { exportId: result.exportId, delayMs: 0 });
        return;
      }
      dragExportRef.current = next;
      setDragExport(next);
      dragPreparingRef.current = '';
      setDragPreparingPath('');
      setNotice(`${dragText.ready}: ${entry.name}`);
    } catch (reason) {
      if (sequence === dragPreparationSequence.current) {
        dragPreparingRef.current = '';
        setDragPreparingPath('');
      }
      setError(String(reason));
    }
  }

  function startRemoteDrag(event: React.DragEvent, entry: FileEntry, basePath = path) {
    if (!active || entry.file_type === 'Symlink') return;
    cancelScheduledDragPreparation();
    event.preventDefault();
    event.stopPropagation();
    const remotePath = entryRemotePath(basePath, entry);
    const prepared = dragExportRef.current;
    if (!prepared || prepared.connectionId !== active.id || prepared.remotePath !== remotePath) {
      const preparing = dragPreparingRef.current === remotePath;
      setNotice(preparing ? dragText.retry : `${dragText.preparing}: ${entry.name}`);
      if (!preparing) void prepareRemoteDrag(entry, basePath);
      return;
    }
    let finished = false;
    void startDrag({ item: [prepared.localPath], icon: prepared.iconPath, mode: 'copy' }, (payload) => {
      if (finished) return;
      finished = true;
      setNotice(`${payload.result === 'Dropped' ? dragText.copied : dragText.cancelled}: ${entry.name}`);
      void cleanupDragExport(prepared, payload.result === 'Dropped' ? 30 * 60 * 1000 : 0);
    }).catch((reason) => { setError(String(reason)); void cleanupDragExport(prepared); });
  }

  async function closeRemoteEdit(edit: RemoteEdit) {
    try {
      await invoke('remote_edit_close', { editId: edit.editId });
      setRemoteEdits((current) => current.filter((item) => item.editId !== edit.editId));
    } catch (reason) { setError(String(reason)); }
  }

  function openEntryContext(event: React.MouseEvent, entry: FileEntry, basePath = path) {
    if (!active) return;
    event.preventDefault();
    event.stopPropagation();
    setBrowserContextMenu(null);
    setColumnMenu(null);
    cancelScheduledDragPreparation();
    const remotePath = entryRemotePath(basePath, entry);
    if (!selectedRemoteItems.some((item) => item.connectionId === active.id && item.remotePath === remotePath)) {
      const item = selectedItem(entry, basePath);
      if (item) setSelectedRemoteItems([item]);
    }
    const target = event.currentTarget as HTMLElement;
    const rect = target.getBoundingClientRect();
    const requestedX = event.clientX || rect.right;
    const requestedY = event.clientY || rect.bottom;
    setEntryContextMenu({
      x: Math.max(8, Math.min(requestedX, window.innerWidth - 230)),
      y: Math.max(8, Math.min(requestedY, window.innerHeight - 330)),
      entry,
      basePath,
    });
  }

  function openBrowserContext(event: React.MouseEvent, basePath = path) {
    if (!active) return;
    event.preventDefault();
    event.stopPropagation();
    cancelScheduledDragPreparation();
    setEntryContextMenu(null);
    setColumnMenu(null);
    setBrowserContextMenu({
      x: Math.max(8, Math.min(event.clientX, window.innerWidth - 210)),
      y: Math.max(8, Math.min(event.clientY, window.innerHeight - 80)),
      basePath,
    });
  }

  function informationTargets(entry: FileEntry, basePath: string) {
    const clicked = selectedItem(entry, basePath);
    if (!clicked || !active) return [];
    return selectedRemoteItems.some((item) => item.connectionId === active.id && item.remotePath === clicked.remotePath) ? selectedRemoteItems.filter((item) => item.connectionId === active.id) : [clicked];
  }

  function openFileInformation(event: React.MouseEvent, entry: FileEntry, basePath = path) {
    if (!active) return;
    event.preventDefault();
    event.stopPropagation();
    cancelScheduledDragPreparation();
    const items = informationTargets(entry, basePath);
    if (items.length === 0) return;
    setSelectedRemoteItems(items);
    setEntryContextMenu(null);
    setFileInformationTarget({ items, focus: 'metadata' });
  }

  async function deleteEntry(entry: FileEntry, basePath: string) {
    if (!active) return;
    setEntryContextMenu(null);
    if (preferences.confirmDelete && !window.confirm(p2.deleteConfirm)) return;
    try {
      await invoke('remote_delete', { request: { connectionId: active.id, path: entryRemotePath(basePath, entry), isDirectory: entry.file_type === 'Directory' } });
      await loadDirectory(active, basePath);
    } catch (reason) { setError(String(reason)); }
  }

  function placeOnRemoteClipboard(entry: FileEntry, basePath: string, mode: 'cut' | 'copy') {
    if (!active || entry.file_type === 'Symlink') return;
    setRemoteClipboard({ connectionId: active.id, sourcePath: entryRemotePath(basePath, entry), entry, mode });
    setEntryContextMenu(null);
    setNotice(`${mode === 'cut' ? menuText.cutReady : menuText.copied}: ${entry.name}`);
  }

  async function pasteRemote(basePath: string) {
    if (!active || !remoteClipboard || remoteClipboard.connectionId !== active.id) return;
    setEntryContextMenu(null);
    const sourceParent = remoteClipboard.sourcePath.slice(0, remoteClipboard.sourcePath.lastIndexOf('/')) || '/';
    if (remoteClipboard.mode === 'cut' && sourceParent === basePath) {
      setRemoteClipboard(null);
      setNotice(`${menuText.pasted}: ${remoteClipboard.entry.name}`);
      return;
    }
    const targetEntries = basePath === path ? entries : columnLevels.find((level) => level.path === basePath)?.entries ?? [];
    const occupied = new Set(targetEntries.map((entry) => entry.name));
    let destinationName = remoteClipboard.entry.name;
    if (occupied.has(destinationName)) destinationName = nextCopyName(destinationName, remoteClipboard.entry.file_type === 'Directory', occupied);
    try {
      await invoke('remote_paste', { request: { connectionId: active.id, sourcePath: remoteClipboard.sourcePath, destinationPath: joinPath(basePath, destinationName), isDirectory: remoteClipboard.entry.file_type === 'Directory', cut: remoteClipboard.mode === 'cut' } });
      if (remoteClipboard.mode === 'cut') setRemoteClipboard(null);
      setNotice(`${menuText.pasted}: ${destinationName}`);
      await loadDirectory(active, basePath);
    } catch (reason) { setError(String(reason)); }
  }

  const densityMetrics = fileDensityMetrics[preferences.fileRowDensity];
  return <main className={`app-shell ${transferPanelCollapsed ? 'queue-collapsed' : ''}`} style={{ '--file-name-font-size': `${preferences.fileNameFontSize}px`, '--file-name-font-weight': preferences.fileNameFontWeight === 'normal' ? 400 : 650, '--file-row-height': `${densityMetrics.list}px`, '--column-row-height': `${densityMetrics.column}px`, '--icon-row-height': `${densityMetrics.icon}px` } as React.CSSProperties}>
    <header className="toolbar" onPointerDown={startWindowDrag}>
      <div className="brand"><Cloud size={21}/><span>{t.title}</span></div>
      <button className="icon-button" aria-label={sidebarCollapsed ? queueText.showSidebar : queueText.hideSidebar} title={sidebarCollapsed ? queueText.showSidebar : queueText.hideSidebar} aria-expanded={!sidebarCollapsed} onClick={() => setSidebarCollapsed((current) => !current)}>{sidebarCollapsed ? <PanelLeftOpen size={17}/> : <PanelLeftClose size={17}/>}</button>
      <button className="toolbar-action" aria-label={`${windowCopy[language].newWindow} (⌘N)`} title={`${windowCopy[language].newWindow} (⌘N)`} onClick={() => void openNewWindow()}><AppWindow size={16}/><span className="toolbar-action-label">{windowCopy[language].newWindow}</span></button>
      <button className="primary toolbar-action" aria-label={t.connect} title={t.connect} onClick={() => { setConnectingBookmark(null); setSelectedKeyPath(''); setConnectSheetMode('connect'); setShowConnect(true); }}><Plus size={16}/><span className="toolbar-action-label">{t.connect}</span></button>
      <button className="toolbar-action" aria-label={t.keys} title={t.keys} onClick={() => void openKeyManagerWindow()}><KeyRound size={16}/><span className="toolbar-action-label">{t.keys}</span></button>
      <button className="toolbar-action" aria-label={t.settings} title={t.settings} onClick={() => setShowPreferences(true)}><Settings size={16}/><span className="toolbar-action-label">{t.settings}</span></button>
      <div className="toolbar-spacer" />
      <label className="language"><span>Language</span><select value={language} onChange={(event) => setPreferences((current) => ({ ...current, language: event.target.value as Language }))}><option value="ja">日本語</option><option value="en">English</option><option value="zh-CN">简体中文</option></select></label>
    </header>
    <section className={`workspace ${sidebarCollapsed ? 'sidebar-collapsed' : ''}`} style={{ '--sidebar-width': `${sidebarWidth}px` } as React.CSSProperties}>
      <aside className="sidebar">
        <div className="sidebar-section-heading bookmarks-heading">
          <div className="sidebar-label">{t.bookmarks}</div>
          <button className="icon-button" aria-label={t.importBookmarks} title={t.importBookmarks} onClick={() => void importBookmarks()}><FileUp size={14}/></button>
          <button className="icon-button" aria-label={t.exportBookmarks} title={t.exportBookmarks} disabled={connections.length === 0} onClick={() => void exportBookmarks()}><FileDown size={14}/></button>
        </div>
        {availableTags.length > 0 && <div className="tag-filter"><button className={!selectedTag ? 'active' : ''} onClick={() => setSelectedTag('')}>{p1.all}</button>{availableTags.map((tag) => <button key={tag} className={selectedTag === tag ? 'active' : ''} onClick={() => setSelectedTag(tag)}>{tag}</button>)}</div>}
        {connections.length === 0 && <p className="muted">No saved connections</p>}
        {visibleConnections.map((connection) => <div
          className={`bookmark-row ${draggedBookmarkId === connection.id ? 'is-dragging' : ''} ${bookmarkDropTarget?.id === connection.id ? `drop-${bookmarkDropTarget.edge}` : ''}`}
          key={connection.id}
          data-bookmark-id={connection.id}
        >
          <button
            type="button"
            className="bookmark-drag-handle"
            disabled={Boolean(selectedTag)}
            aria-label={bookmarkOrderText.handle.replace('{{name}}', connection.name)}
            title={selectedTag ? bookmarkOrderText.filtered : bookmarkOrderText.hint}
            onPointerDown={(event) => startBookmarkPointerDrag(event, connection.id)}
            onPointerMove={moveBookmarkPointerDrag}
            onPointerUp={endBookmarkPointerDrag}
            onPointerCancel={(event) => { if (bookmarkPointerDragRef.current?.pointerId === event.pointerId) finishBookmarkDrag(); }}
            onClick={(event) => { event.preventDefault(); event.stopPropagation(); }}
            onKeyDown={(event) => {
              if (event.key === 'ArrowUp') { event.preventDefault(); moveBookmarkWithKeyboard(connection.id, -1); }
              if (event.key === 'ArrowDown') { event.preventDefault(); moveBookmarkWithKeyboard(connection.id, 1); }
            }}
          ><GripVertical size={14}/></button>
          <button className={`bookmark bookmark-main ${active?.id === connection.id ? 'selected' : ''}`} onClick={() => { if (active?.id === connection.id) { void loadDirectory(connection, path); } else { setConnectingBookmark(connection); setSelectedKeyPath(connection.keyPath ?? ''); setConnectSheetMode('connect'); setShowConnect(true); } }}><HardDrive size={16}/><span>{connection.name}</span><small>{protocolLabel(connection.protocol)}</small></button>
          <button className="bookmark-edit-button" aria-label={`${t.editBookmark}: ${connection.name}`} title={t.editBookmark} onClick={() => { setConnectingBookmark(connection); setSelectedKeyPath(connection.keyPath ?? ''); setConnectSheetMode('edit'); setShowConnect(true); }}><Pencil size={14}/></button>
        </div>)}
        <div className="sidebar-section-heading history-label"><div className="sidebar-label">{t.history}</div>{history.length > 0 && <button className="icon-button" aria-label={queueText.clearConnectionHistory} title={queueText.clearConnectionHistory} onClick={() => void clearConnectionHistory()}><Trash2 size={14}/></button>}</div>
        {history.length === 0 ? <p className="muted">{p1.noHistory}</p> : history.slice(0, 6).map((item, index) => <button className="bookmark history-item" key={`${item.bookmarkId}-${item.connectedAt}-${index}`} onClick={() => { const saved = connections.find((connection) => connection.id === item.bookmarkId); const connection = saved ?? { id: item.bookmarkId, name: item.name, protocol: item.protocol, host: item.host, port: item.port, username: item.username, initialPath: '/', tags: '' }; setConnectingBookmark(connection); setSelectedKeyPath(connection.keyPath ?? ''); setConnectSheetMode('connect'); setShowConnect(true); }}><HardDrive size={14}/><span>{item.name}</span><small>{item.connectedAt.slice(5, 16)}</small></button>)}
      </aside>
      <div className="sidebar-resizer" role="separator" aria-label={queueText.resizeSidebar} title={queueText.resizeSidebar} aria-orientation="vertical" aria-valuemin={minimumSidebarWidth} aria-valuemax={maximumSidebarWidth} aria-valuenow={sidebarWidth} tabIndex={0} onPointerDown={startSidebarResize} onDoubleClick={() => setSidebarWidth(defaultSidebarWidth)} onKeyDown={(event) => { if (event.key === 'ArrowLeft') { event.preventDefault(); adjustSidebarWidth(-10); } if (event.key === 'ArrowRight') { event.preventDefault(); adjustSidebarWidth(10); } if (event.key === 'Home') { event.preventDefault(); setSidebarWidth(defaultSidebarWidth); } }}/>
      <section className={`browser ${isDragOver ? 'drag-over' : ''}`} ref={browserZoneRef}>
        {notice && <div className="notice-banner" role="status" aria-live="polite">{notice}</div>}
        {active ? <>
          <div className="browser-toolbar">
            <button aria-label={t.refresh} onClick={() => void loadDirectory()}><RefreshCw size={17} className={busy ? 'spinning' : ''}/></button>
            <div className="search"><Search size={16}/><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder={t.search}/></div>
            <div className="view-switcher" role="group" aria-label={t.name}>
              <button className={viewMode === 'list' ? 'active' : ''} aria-label={viewsText.list} title={viewsText.list} aria-pressed={viewMode === 'list'} onClick={() => selectViewMode('list')}><List size={16}/></button>
              <button className={viewMode === 'icons' ? 'active' : ''} aria-label={viewsText.icons} title={viewsText.icons} aria-pressed={viewMode === 'icons'} onClick={() => selectViewMode('icons')}><Grid2X2 size={16}/></button>
              <button className={viewMode === 'columns' ? 'active' : ''} aria-label={viewsText.columns} title={viewsText.columns} aria-pressed={viewMode === 'columns'} onClick={() => selectViewMode('columns')}><Columns3 size={16}/></button>
            </div>
            <div className="browser-toolbar-spacer" />
            <button className="browser-toolbar-action" aria-label={syncText.button} title={syncText.button} onClick={() => void openSyncPreview()}><FolderSync size={17}/><span className="browser-toolbar-action-label">{syncText.button}</span></button>
            <button className="browser-toolbar-action" aria-label={t.newFolder} disabled={active.protocol === 's3' && !active.s3PreserveEmptyDirectories} title={active.protocol === 's3' ? s3Copy[language].readOnly : t.newFolder} onClick={() => void createDirectory()}><FolderPlus size={17}/><span className="browser-toolbar-action-label">{t.newFolder}</span></button>
            <button className="browser-toolbar-action" aria-label={t.uploadFolder} title={t.uploadFolder} onClick={() => void uploadDirectory()}><FolderUp size={16}/><span className="browser-toolbar-action-label">{t.uploadFolder}</span></button>
            <button className="primary browser-toolbar-action" aria-label={t.upload} title={t.upload} onClick={() => void uploadFiles()}><Upload size={16}/><span className="browser-toolbar-action-label">{t.upload}</span></button>
          </div>
          <div className="path-toolbar">
            <button aria-label={accessibilityCopy[language].back} title={accessibilityCopy[language].back} disabled={directoryHistoryIndex <= 0} onClick={() => void navigateDirectoryHistory(-1)}><ChevronLeft size={18}/></button><button aria-label={accessibilityCopy[language].forward} title={accessibilityCopy[language].forward} disabled={directoryHistoryIndex < 0 || directoryHistoryIndex >= directoryHistory.length - 1} onClick={() => void navigateDirectoryHistory(1)}><ChevronRight size={18}/></button>
            <button className="parent-directory-button" aria-label={accessibilityCopy[language].parent} title={accessibilityCopy[language].parent} disabled={path === '/'} onClick={() => void navigateDirectory(active, parentPath(path))}><ArrowUpToLine size={18}/></button>
            <div className="path-field"><span>{t.path}</span><input ref={pathInputRef} value={pathDraft} title={visibleRemotePath(path)} onChange={(event) => setPathDraft(event.target.value)} onBlur={() => setPathDraft(visibleRemotePath(path))} onKeyDown={(event) => { if (event.key === 'Enter') { event.preventDefault(); const requestedPath = pathDraft === visibleRemotePath(path) ? path : pathDraft; void navigateDirectory(active, requestedPath); } if (event.key === 'Escape') { event.preventDefault(); setPathDraft(visibleRemotePath(path)); event.currentTarget.blur(); } }} /></div>
            <button className="copy-path-button" aria-label={t.copyPath} title={t.copyPath} onClick={() => void copyCurrentPath()}>{pathCopied ? <Check size={17}/> : <Copy size={17}/>}</button>
          </div>
          {error && <div className="error-banner"><strong>{t.error}:</strong> {error}</div>}
          <div className="connection-strip"><span className="online-dot" />{active.name}<span>·</span><span>{active.username}@{active.host}</span><span className="connected">{t.connected}</span></div>
          <div className="file-display-region" onContextMenu={(event) => openBrowserContext(event)}>
          {isDragOver && <div className="drop-overlay"><Upload size={32}/><strong>{p2.drop}</strong></div>}
          {viewMode === 'list' && <div className="file-table" role="table" style={{ '--file-columns': `${visibleColumns.map((column) => `${columnWidths[column]}px`).join(' ')} minmax(0, 1fr) 30px`, '--file-min-width': `${visibleColumns.reduce((total, column) => total + columnWidths[column], 0) + 48 + visibleColumns.length * 8}px` } as React.CSSProperties}>
            <div className="file-header" role="row" onContextMenu={openColumnVisibilityMenu}>
              {visibleColumns.map((column) => <ResizableColumnHeader key={column} label={columnLabel(column)} column={column} width={columnWidths[column]} resizeLabel={columnsText.resize} sorted={sortColumn === column} direction={sortDirection} onSort={sortByColumn} onStart={startColumnResize} onAdjust={adjustColumnWidth}/>)}<span className="file-grid-spacer" aria-hidden="true"/><span className="actions-column-heading" role="columnheader" />
            </div>
            {filteredEntries.map((entry) => { const remotePath = entryRemotePath(path, entry); const selected = selectedRemoteItems.some((item) => item.connectionId === active.id && item.remotePath === remotePath); const renaming = isInlineRenaming(entry, path); const ready = dragExport?.connectionId === active.id && dragExport.remotePath === remotePath; return <div className={`file-row interactive ${selected ? 'selected' : ''} ${ready ? 'drag-ready' : ''}`} key={entryIdentity(entry)} role="row" aria-selected={selected} tabIndex={0} draggable={entry.file_type !== 'Symlink' && !renaming} onContextMenu={(event) => openEntryContext(event, entry)} onClick={(event) => scheduleRemoteDragPreparation(event, entry)} onKeyDown={(event) => { if (event.key === 'F2') { beginInlineRename(event, entry, path); return; } if (event.key === ' ') { event.preventDefault(); scheduleRemoteDragPreparation(event, entry); } if (event.key === 'Enter') { event.preventDefault(); entry.file_type === 'Directory' ? void navigateDirectory(active, remotePath) : void editRemoteFile(entry); } }} onDragStart={(event) => startRemoteDrag(event, entry)} onDoubleClick={() => { cancelScheduledDragPreparation(); entry.file_type === 'Directory' ? void navigateDirectory(active, remotePath) : void editRemoteFile(entry); }}>
              {visibleColumns.map((column) => <span className={`list-cell ${column}-column`} role="cell" key={column}>{renderListCell(entry, column, path, selected)}</span>)}<span className="file-grid-spacer" aria-hidden="true"/><button aria-label={menuText.information} title={menuText.information} onClick={(event) => openFileInformation(event, entry)}><MoreHorizontal size={18}/></button>
            </div>; })}
          </div>}
          {viewMode === 'icons' && <div className="icon-grid" role="grid">
            {filteredEntries.length === 0 && <p className="view-empty">{viewsText.empty}</p>}
            {filteredEntries.map((entry) => { const remotePath = entryRemotePath(path, entry); const selected = selectedRemoteItems.some((item) => item.connectionId === active.id && item.remotePath === remotePath); const renaming = isInlineRenaming(entry, path); const ready = dragExport?.connectionId === active.id && dragExport.remotePath === remotePath; return <div className={`icon-entry ${selected ? 'selected' : ''} ${ready ? 'drag-ready' : ''}`} role="gridcell" aria-selected={selected} key={entryIdentity(entry)} draggable={entry.file_type !== 'Symlink' && !renaming} onContextMenu={(event) => openEntryContext(event, entry)} onDragStart={(event) => startRemoteDrag(event, entry)}>
              {renaming ? <div className="icon-entry-main inline-renaming">
                <RemoteEntryIcon entry={entry} className="entry-art" size={entry.file_type === 'Directory' ? 46 : 44}/>
                {inlineRenameInput('icons')}<small>{entry.file_type === 'Directory' ? entry.permissions ?? '—' : formatBytes(entry.size)}</small>
              </div> : <button className="icon-entry-main" title={selected ? menuText.rename : entry.name} onClick={(event) => { if (selected) beginInlineRename(event, entry, path); else scheduleRemoteDragPreparation(event, entry); }} onDoubleClick={() => { cancelScheduledDragPreparation(); entry.file_type === 'Directory' ? void navigateDirectory(active, remotePath) : void editRemoteFile(entry); }} onKeyDown={(event) => { if (event.key === 'F2') { beginInlineRename(event, entry, path); return; } if (event.key === ' ') { event.preventDefault(); scheduleRemoteDragPreparation(event, entry); } if (event.key !== 'Enter') return; event.preventDefault(); entry.file_type === 'Directory' ? void navigateDirectory(active, remotePath) : void editRemoteFile(entry); }}>
                <RemoteEntryIcon entry={entry} className="entry-art" size={entry.file_type === 'Directory' ? 46 : 44}/>
                <strong>{entry.name}</strong><small>{entry.file_type === 'Directory' ? entry.permissions ?? '—' : formatBytes(entry.size)}</small>
              </button>}
              <button className="icon-entry-more" aria-label={menuText.information} title={menuText.information} onClick={(event) => openFileInformation(event, entry)}><MoreHorizontal size={16}/></button>
            </div>; })}
          </div>}
          {viewMode === 'columns' && <div className="column-browser" role="listbox" aria-label={viewsText.columns}>
            {columnLevels.map((level, levelIndex) => {
              const visibleLevelEntries = level.entries.filter((entry) => (preferences.showHiddenFiles || !entry.name.startsWith('.')) && entry.name.toLowerCase().includes(query.toLowerCase()));
              return <section className="directory-column" key={`${level.path}-${levelIndex}`} aria-label={level.path} onContextMenu={(event) => openBrowserContext(event, level.path)}>
                <div className="directory-column-title" title={visibleRemotePath(level.path)}>{visibleRemotePath(level.path)}</div>
                {visibleLevelEntries.length === 0 && <p className="view-empty">{viewsText.empty}</p>}
                {visibleLevelEntries.map((entry) => { const remotePath = entryRemotePath(level.path, entry); const selectedForAction = selectedRemoteItems.some((item) => item.connectionId === active.id && item.remotePath === remotePath); const selected = level.selectedName === entryIdentity(entry) || selectedForAction; const renaming = isInlineRenaming(entry, level.path); const ready = dragExport?.connectionId === active.id && dragExport.remotePath === remotePath; return <div className={`column-entry ${selected ? 'selected' : ''} ${ready ? 'drag-ready' : ''}`} key={entryIdentity(entry)} role="option" aria-selected={selected} draggable={entry.file_type !== 'Symlink' && !renaming} onContextMenu={(event) => openEntryContext(event, entry, level.path)} onDragStart={(event) => startRemoteDrag(event, entry, level.path)}>
                  {renaming ? <div className="column-entry-main inline-renaming"><RemoteEntryIcon entry={entry} size={entry.file_type === 'Directory' ? 17 : 16}/>{inlineRenameInput('columns')}</div> : <button className="column-entry-main" title={selectedForAction ? menuText.rename : entry.name} onClick={(event) => { if (selectedForAction) { beginInlineRename(event, entry, level.path); return; } if (event.detail <= 1) { cancelScheduledDragPreparation(); selectRemoteEntry(event, entry, level.path, visibleLevelEntries); if (!event.metaKey && !event.ctrlKey && !event.shiftKey) void openColumnEntry(levelIndex, entry); } }} onDoubleClick={() => { cancelScheduledDragPreparation(); if (entry.file_type !== 'Directory') void editRemoteFile(entry, level.path); }} onKeyDown={(event) => { if (event.key === 'F2') { beginInlineRename(event, entry, level.path); return; } if (event.key !== 'Enter') return; event.preventDefault(); entry.file_type === 'Directory' ? void openColumnEntry(levelIndex, entry) : void editRemoteFile(entry, level.path); }}>
                    <RemoteEntryIcon entry={entry} size={entry.file_type === 'Directory' ? 17 : 16}/><span>{entry.name}</span>{entry.file_type === 'Directory' ? <ChevronRight size={14}/> : <small>{formatBytes(entry.size)}</small>}
                  </button>}
                  <button className="column-entry-more" aria-label={menuText.information} title={menuText.information} onClick={(event) => openFileInformation(event, entry, level.path)}><MoreHorizontal size={15}/></button>
                </div>; })}
              </section>;
            })}
          </div>}
          </div>
          <nav className="breadcrumb-bar" aria-label={t.breadcrumbs}>
            {breadcrumbs.map((crumb, index) => <span className="breadcrumb-item" key={crumb.path}>
              {index > 0 && <ChevronRight size={13} aria-hidden="true"/>}
              <button title={crumb.path} aria-current={index === breadcrumbs.length - 1 ? 'location' : undefined} onClick={() => void navigateDirectory(active, crumb.path)}>{crumb.label}</button>
            </span>)}
          </nav>
        </> : <div className="welcome"><Cloud size={46}/><h1>{t.empty}</h1><p>{t.emptyDetail}</p><button className="primary" onClick={() => { setConnectingBookmark(null); setSelectedKeyPath(''); setConnectSheetMode('connect'); setShowConnect(true); }}>{t.connect}</button></div>}
      </section>
    </section>
    <section className={`transfer-panel ${transferPanelCollapsed ? 'collapsed' : ''}`}>
      <div className="transfer-heading"><span>{t.status}</span><small>{transfers.filter((item) => item.status === 'Running').length + remoteEdits.length + (dragPreparingPath ? 1 : 0)} active</small><span className="transfer-heading-spacer"/>{transfers.some((item) => item.status !== 'Running') && <button className="icon-button" aria-label={queueText.clearTransferHistory} title={queueText.clearTransferHistory} onClick={() => void clearTransferHistory()}><Trash2 size={15}/></button>}<button className="icon-button" aria-label={transferPanelCollapsed ? queueText.expand : queueText.collapse} title={transferPanelCollapsed ? queueText.expand : queueText.collapse} aria-expanded={!transferPanelCollapsed} onClick={toggleTransferPanel}>{transferPanelCollapsed ? <ChevronUp size={17}/> : <ChevronDown size={17}/>}</button></div>
      <div className="transfer-list">
        {directoryProgress && directoryProgress.status !== 'completed' && <div className="directory-progress"><span>{directoryProgress.completedFiles} / {directoryProgress.totalFiles || '…'}</span><strong>{directoryProgress.status === 'reconnecting' ? queueText.reconnecting : directoryProgress.status === 'queued' ? queueText.queued : directoryProgress.currentPath}</strong><button onClick={() => void controlDirectoryTransfer(directoryPaused ? 'resume' : 'pause')}>{directoryPaused ? t.resume : t.pause}</button><button onClick={() => void controlDirectoryTransfer('cancel')}>{t.cancel}</button></div>}
        {dragPreparingPath && <div className="drag-export-progress"><LoaderCircle className="spinning" size={15}/><strong>{dragPreparingPath.split('/').pop()}</strong><span>{dragText.preparing}</span></div>}
        {remoteEdits.map((edit) => <div className={`remote-edit-row ${edit.status}`} key={edit.editId}><Pencil size={15}/><strong>{edit.name}</strong><span>{edit.status === 'waiting' ? editText.waiting : edit.status === 'failed' ? edit.detail : editText.watching}</span><small title={edit.remotePath}>{edit.remotePath}</small><button className="icon-button" aria-label={editText.stop} title={editText.stop} onClick={() => void closeRemoteEdit(edit)}><Trash2 size={14}/></button></div>)}
        {transfers.length === 0 && remoteEdits.length === 0 && !dragPreparingPath ? <p className="muted">Transfers will appear here.</p> : transfers.slice(0, 8).map((transfer) => <div className="transfer-row" key={transfer.id}>
          <span className={`transfer-status ${transfer.status.toLowerCase()}`}/><strong>{transfer.name}</strong>
          <span>{transfer.totalBytes ? `${Math.round(((transfer.transferredBytes ?? 0) / transfer.totalBytes) * 100)}%` : transfer.direction}</span>
          <span>{transfer.status === 'Running' && transfer.activity === 'queued' ? queueText.queued : transfer.status === 'Running' && transfer.activity === 'reconnecting' ? queueText.reconnecting : transfer.speed ? `${formatBytes(transfer.speed)}/s · ${p2.eta} ${formatDuration(transfer.etaSeconds)}` : transfer.detail}</span>
          <span className="transfer-actions">
            {transfer.status === 'Running' && <><button onClick={() => void controlTransfer(transfer, pausedTransfers.has(transfer.id) ? 'resume' : 'pause')}>{pausedTransfers.has(transfer.id) ? t.resume : t.pause}</button><button onClick={() => void controlTransfer(transfer, 'cancel')}>{t.cancel}</button></>}
            {transfer.status === 'Failed' && transfer.localPath ? transfer.connectionId === active?.id ? <button onClick={() => void retryTransfer(transfer)}>{t.retry}</button> : <span title={queueText.reconnectToRetry}>{transfer.status}</span> : transfer.status !== 'Running' ? transfer.status : null}
          </span>
        </div>)}
      </div>
    </section>
    {columnMenu && <div className="column-visibility-menu" role="menu" style={{ left: columnMenu.x, top: columnMenu.y }} onContextMenu={(event) => event.preventDefault()}>
      <div className="context-menu-heading">{columnsText.displayColumns}</div>
      <button type="button" role="menuitemcheckbox" aria-checked="true" disabled><Check size={14}/>{t.name}</button>
      {optionalColumnOrder.map((column) => <button type="button" role="menuitemcheckbox" aria-checked={columnVisibility[column]} key={column} onClick={() => toggleColumn(column)}>{columnVisibility[column] ? <Check size={14}/> : <span className="menu-check-placeholder"/>}{columnLabel(column)}</button>)}
      <button type="button" role="menuitemcheckbox" aria-checked="true" disabled><Check size={14}/>{accessibilityCopy[language].more}</button>
    </div>}
    {browserContextMenu && active && <div className="entry-context-menu browser-context-menu" role="menu" style={{ left: browserContextMenu.x, top: browserContextMenu.y }} onContextMenu={(event) => event.preventDefault()}>
      <button type="button" role="menuitem" disabled={active.protocol === 's3' && !active.s3PreserveEmptyDirectories} onClick={() => { const targetPath = browserContextMenu.basePath; setBrowserContextMenu(null); void createDirectory(targetPath); }}><FolderPlus size={15}/>{browserMenuText.newDirectory}</button>
    </div>}
    {entryContextMenu && active && <div className="entry-context-menu" role="menu" style={{ left: entryContextMenu.x, top: entryContextMenu.y }} onContextMenu={(event) => event.preventDefault()}>
      <div className="context-menu-heading" title={entryContextMenu.entry.name}>{entryContextMenu.entry.name}</div>
      <button role="menuitem" disabled={entryContextMenu.entry.file_type === 'Symlink'} onClick={() => placeOnRemoteClipboard(entryContextMenu.entry, entryContextMenu.basePath, 'cut')}><Scissors size={15}/>{menuText.cut}</button>
      <button role="menuitem" disabled={entryContextMenu.entry.file_type === 'Symlink'} onClick={() => placeOnRemoteClipboard(entryContextMenu.entry, entryContextMenu.basePath, 'copy')}><Copy size={15}/>{menuText.copy}</button>
      <button role="menuitem" disabled={!remoteClipboard || remoteClipboard.connectionId !== active.id} onClick={() => void pasteRemote(entryContextMenu.basePath)}><ClipboardPaste size={15}/>{menuText.paste}</button>
      <button className="danger" role="menuitem" onClick={() => void deleteEntry(entryContextMenu.entry, entryContextMenu.basePath)}><Trash2 size={15}/>{menuText.delete}</button>
      <div className="context-menu-separator" role="separator"/>
      <button role="menuitem" disabled={entryContextMenu.entry.file_type !== 'File'} onClick={() => { const target = entryContextMenu; setEntryContextMenu(null); void downloadFile(target.entry, target.basePath); }}><Download size={15}/>{menuText.download}</button>
      <button role="menuitem" onClick={() => { const items = informationTargets(entryContextMenu.entry, entryContextMenu.basePath); if (items.length) { setSelectedRemoteItems(items); setFileInformationTarget({ items, focus: 'metadata' }); } setEntryContextMenu(null); }}><FileCog size={15}/>{menuText.information}</button>
    </div>}
    {fileInformationTarget && active && <FileInformationSheet connection={active} target={fileInformationTarget} t={t} text={menuText} onClose={() => setFileInformationTarget(null)} onSave={async (name, permissions, modified, ownerId, groupId) => {
      if (permissions !== null || modified !== null || ownerId !== null || groupId !== null) {
        for (const item of fileInformationTarget.items) await invoke('remote_set_metadata', { request: { connectionId: active.id, path: item.remotePath, permissions, modified, ownerId, groupId } });
      }
      const singleItem = fileInformationTarget.items.length === 1 ? fileInformationTarget.items[0] : null;
      if (singleItem && name !== null && name !== singleItem.entry.name) {
        const destinationPath = joinPath(singleItem.basePath, name);
        await invoke('remote_rename', { request: { connectionId: active.id, oldPath: singleItem.remotePath, newPath: destinationPath } });
      }
      setFileInformationTarget(null);
      setSelectedRemoteItems([]);
      setNotice(menuText.saved);
      await loadDirectory(active, path);
    }}
    />}
    {dropConflict && <div className="modal-backdrop drop-conflict-backdrop" role="presentation"><section className="connect-sheet drop-conflict-sheet" role="dialog" aria-modal="true" aria-labelledby="drop-conflict-title">
      <header><div><h2 id="drop-conflict-title">{dropConflictText.title}</h2><p>{(dropConflict.incomingIsDirectory === dropConflict.existingIsDirectory ? (dropConflict.incomingIsDirectory ? dropConflictText.folderDetail : dropConflictText.fileDetail) : dropConflictText.typeMismatchDetail).replace('{{name}}', dropConflict.name)}</p></div></header>
      {dropConflict.incomingIsDirectory && dropConflict.existingIsDirectory ? <div className="drop-conflict-options">
        <button type="button" className="drop-conflict-option" onClick={() => settleDropConflict('merge')}><strong>{dropConflictText.merge}</strong><span>{dropConflictText.mergeDetail}</span></button>
        <button type="button" className="drop-conflict-option danger" onClick={() => settleDropConflict('replace')}><strong>{dropConflictText.replace}</strong><span>{dropConflictText.replaceDetail}</span></button>
      </div> : <div className="form-actions"><button type="button" onClick={() => settleDropConflict('cancel')}>{dropConflictText.cancel}</button><button type="button" className="primary" onClick={() => settleDropConflict('overwrite')}>{dropConflictText.overwrite}</button></div>}
      {dropConflict.incomingIsDirectory && dropConflict.existingIsDirectory && <div className="form-actions"><button type="button" onClick={() => settleDropConflict('cancel')}>{dropConflictText.cancel}</button></div>}
    </section></div>}
    {showConnect && <ConnectSheet mode={connectSheetMode} bookmark={connectingBookmark} initialKeyPath={selectedKeyPath} defaultProtocol={preferences.defaultProtocol} googleClientId={preferences.googleClientId} googleExportFormats={preferences} t={t} phaseCopy={p1} passphraseText={sshPassphrasePromptCopy[language]} localCopy={bookmarkLocalText} s3Text={s3Copy[language]} sambaText={sambaCopy[language]} googleText={googleDriveCopy[language]} cloudFtpText={cloudFtpCopy[language]} transferText={transferSettingsCopy[language]} onClose={() => setShowConnect(false)} onSaved={(connection) => {
      setConnections((current) => upsertConnectionInOrder(current, connection));
      setActive((current) => {
        if (current?.id !== connection.id) return current;
        const targetChanged = connectionTargetChanged(current, connection);
        return targetChanged ? null : { ...current, name: connection.name, initialPath: connection.initialPath, keyPath: connection.keyPath, keyPassphraseNotRequired: connection.keyPassphraseNotRequired, localDirectory: connection.localDirectory, tags: connection.tags, transferMaxConcurrent: connection.transferMaxConcurrent, transferBandwidthLimitKbps: connection.transferBandwidthLimitKbps, transferRetryCount: connection.transferRetryCount };
      });
      setNotice(t.bookmarkSaved);
      setShowConnect(false);
    }} onConnected={(connection) => { setConnections((current) => upsertConnectionInOrder(current, connection)); void invoke('bookmark_save', { bookmark: connection }).then(() => invoke('connection_history_record', { bookmark: connection })).then(() => invoke<ConnectionHistory[]>('connection_history_list')).then(setHistory).catch((reason) => setError(invokeErrorMessage(reason))); setActive(connection); setShowConnect(false); void navigateDirectory(connection, connection.initialPath, true); }} />}
    {showPreferences && <PreferencesSheet value={preferences} language={language} t={t} softwareUpdate={softwareUpdate} onCheckUpdate={() => void checkForSoftwareUpdate(true)} onShowUpdate={() => setShowSoftwareUpdate(true)} onClose={() => setShowPreferences(false)} onSave={(next) => { setPreferences(next); setShowPreferences(false); }} />}
    {showSoftwareUpdate && <SoftwareUpdateSheet state={softwareUpdate} language={language} onClose={() => { if (softwareUpdate.phase !== 'downloading') setShowSoftwareUpdate(false); }} onInstall={() => void downloadAndInstallSoftwareUpdate()} onRestart={() => void relaunch()} />}
    {syncLocalDirectory && <SyncPreviewSheet preview={syncPreview} localDirectory={syncLocalDirectory} remoteDirectory={path} direction={syncDirection} comparison={syncComparison} busy={syncPreviewBusy} executionBusy={syncExecutionBusy} error={syncPreviewError} exclusions={syncExclusions} conflictChoices={syncConflictChoices} progress={syncExecutionProgress} result={syncExecutionResult} history={syncHistory} t={t} text={syncText} onClose={() => { if (syncExecutionBusy) return; setSyncLocalDirectory(''); setSyncPreview(null); }} onDirection={(direction) => { setSyncDirection(direction); setSyncExecutionResult(null); void calculateSyncPreview(syncLocalDirectory, direction); }} onComparison={(comparison) => { setSyncComparison(comparison); setSyncPreview(null); setSyncExecutionResult(null); void calculateSyncPreview(syncLocalDirectory, syncDirection, syncExclusions, comparison); }} onExclusions={(value) => { setSyncExclusions(value); setSyncPreview(null); setSyncExecutionResult(null); }} onConflict={(itemPath, choice) => setSyncConflictChoices((current) => ({ ...current, [itemPath]: choice }))} onRefresh={() => void calculateSyncPreview()} onExecute={() => void executeSync()} onCancelExecution={() => void cancelSyncExecution()} onClearHistory={() => void clearSyncHistory()} />}
  </main>;
}

function FileInformationSheet({ connection, target, t, text, onClose, onSave }: { connection: Connection; target: FileInformationTarget; t: typeof copy[keyof typeof copy]; text: typeof contextMenuCopy[keyof typeof contextMenuCopy]; onClose: () => void; onSave: (name: string | null, permissions: number | null, modified: number | null, ownerId: number | null, groupId: number | null) => Promise<void> }) {
  const entries = target.items.map((item) => item.entry);
  const first = entries[0];
  const multiple = entries.length > 1;
  const samePermissions = entries.every((entry) => entry.permissions === first.permissions);
  const sameModified = entries.every((entry) => entry.modified === first.modified);
  const sameOwner = entries.every((entry) => entry.owner === first.owner);
  const sameGroup = entries.every((entry) => entry.group === first.group);
  const commonPermissions = samePermissions ? first.permissions : undefined;
  const commonModified = sameModified ? first.modified : undefined;
  const commonOwner = sameOwner ? first.owner : undefined;
  const commonGroup = sameGroup ? first.group : undefined;
  const commonKind = entries.every((entry) => entry.file_type === first.file_type) ? first.file_type : undefined;
  const attributesEditable = entries.every((entry) => entry.file_type !== 'Symlink') && (connection.protocol === 'sftp' || connection.protocol === 'ftp' || connection.protocol === 'ftps');
  const modifiedEditable = entries.every((entry) => entry.file_type !== 'Symlink') && (attributesEditable || connection.protocol === 'smb');
  const ownershipEditable = entries.every((entry) => entry.file_type !== 'Symlink') && connection.protocol === 'sftp';
  const [name, setName] = useState(multiple ? '' : first.name);
  const [changePermissions, setChangePermissions] = useState(false);
  const [permissions, setPermissions] = useState(permissionStringToOctal(commonPermissions));
  const [changeModified, setChangeModified] = useState(false);
  const [modified, setModified] = useState(toDateTimeLocal(commonModified));
  const [changeOwnership, setChangeOwnership] = useState(false);
  const [ownerId, setOwnerId] = useState(commonOwner ?? '');
  const [groupId, setGroupId] = useState(commonGroup ?? '');
  const [busy, setBusy] = useState(false);
  const [formError, setFormError] = useState('');

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    const nextName = multiple ? null : name.trim();
    if (!multiple && (!nextName || nextName === '.' || nextName === '..' || nextName.includes('/'))) { setFormError(text.invalidName); return; }
    if ((multiple || nextName === first.name) && !changePermissions && !changeModified && !changeOwnership) { setFormError(text.chooseField); return; }
    let permissionValue: number | null = null;
    if (changePermissions) {
      if (!/^[0-7]{3,4}$/.test(permissions)) { setFormError(text.invalidPermissions); return; }
      permissionValue = Number.parseInt(permissions, 8);
    }
    let modifiedValue: number | null = null;
    if (changeModified) {
      const date = new Date(modified);
      if (!modified || Number.isNaN(date.getTime())) { setFormError(text.invalidDate); return; }
      modifiedValue = Math.floor(date.getTime() / 1000);
    }
    let ownerValue: number | null = null;
    let groupValue: number | null = null;
    if (changeOwnership) {
      const validIdentifier = (value: string) => /^\d+$/.test(value) && Number(value) <= 4_294_967_295;
      if ((!ownerId && !groupId) || (ownerId && !validIdentifier(ownerId)) || (groupId && !validIdentifier(groupId))) { setFormError(text.invalidOwnership); return; }
      ownerValue = ownerId ? Number(ownerId) : null;
      groupValue = groupId ? Number(groupId) : null;
    }
    setBusy(true); setFormError('');
    try { await onSave(nextName, permissionValue, modifiedValue, ownerValue, groupValue); }
    catch (reason) { setFormError(String(reason)); }
    finally { setBusy(false); }
  }

  return <div className="modal-backdrop" role="presentation"><form className="connect-sheet file-information-sheet" onSubmit={submit}>
    <div className="sheet-title"><div><h2>{text.title}</h2><p>{text.detail}</p></div><button type="button" onClick={onClose}>×</button></div>
    <div className="file-information-summary"><FileCog size={28}/><div><strong>{multiple ? text.selectedCount.replace('{{count}}', String(entries.length)) : first.name}</strong><span>{text.kind}: {commonKind ?? text.mixed}</span><span>{multiple || first.file_type === 'Directory' ? '—' : formatBytes(first.size)}</span></div></div>
    {multiple ? <p className="protocol-support-note">{text.multipleName}</p> : <label className="file-information-name">{text.fileName}<input autoFocus={target.focus === 'name'} value={name} onChange={(event) => setName(event.target.value)}/></label>}
    {!attributesEditable && !modifiedEditable && <p className="protocol-support-note">{text.unsupported}</p>}
    {connection.protocol === 'cloudFtp' && <p className="protocol-support-note">{text.cloudFtpSupport}</p>}
    {(connection.protocol === 'ftp' || connection.protocol === 'ftps') && <p className="protocol-support-note">{text.ftpSupport}</p>}
    {connection.protocol === 'smb' && <p className="protocol-support-note">{text.smbSupport}</p>}
    <label className="metadata-toggle"><span><input type="checkbox" disabled={!attributesEditable} checked={changePermissions} onChange={(event) => setChangePermissions(event.target.checked)}/>{text.changePermissions}</span><input aria-label={text.permissions} disabled={!attributesEditable || !changePermissions} value={permissions} onChange={(event) => setPermissions(event.target.value)} placeholder="0644" maxLength={4}/><small>{text.permissions}: {samePermissions ? commonPermissions ?? '—' : text.mixed}</small></label>
    <label className="metadata-toggle"><span><input type="checkbox" disabled={!modifiedEditable} checked={changeModified} onChange={(event) => setChangeModified(event.target.checked)}/>{text.changeModified}</span><input aria-label={text.modified} type="datetime-local" step="1" disabled={!modifiedEditable || !changeModified} value={modified} onChange={(event) => setModified(event.target.value)}/><small>{text.modified}: {sameModified ? commonModified ?? '—' : text.mixed}</small></label>
    <div className="metadata-toggle ownership-toggle"><span><input id="change-ownership" type="checkbox" disabled={!ownershipEditable} checked={changeOwnership} onChange={(event) => setChangeOwnership(event.target.checked)}/><label htmlFor="change-ownership">{text.changeOwnership}</label></span><div className="ownership-fields"><label>{text.owner}<input inputMode="numeric" disabled={!ownershipEditable || !changeOwnership} value={ownerId} onChange={(event) => setOwnerId(event.target.value)} placeholder="UID"/></label><label>{text.group}<input inputMode="numeric" disabled={!ownershipEditable || !changeOwnership} value={groupId} onChange={(event) => setGroupId(event.target.value)} placeholder="GID"/></label></div><small>{text.owner}: {sameOwner ? commonOwner ?? '—' : text.mixed} · {text.group}: {sameGroup ? commonGroup ?? '—' : text.mixed}</small></div>
    {ownershipEditable && <p className="protocol-support-note">{text.ownershipSftp}</p>}
    {formError && <p className="form-error">{formError}</p>}
    <div className="form-actions"><button type="button" onClick={onClose}>{t.cancel}</button><button className="primary" disabled={busy}>{busy && <LoaderCircle className="spinning" size={15}/>} {text.save}</button></div>
  </form></div>;
}

function SyncPreviewSheet({ preview, localDirectory, remoteDirectory, direction, comparison, busy, executionBusy, error, exclusions, conflictChoices, progress, result, history, t, text, onClose, onDirection, onComparison, onExclusions, onConflict, onRefresh, onExecute, onCancelExecution, onClearHistory }: { preview: SyncPreview | null; localDirectory: string; remoteDirectory: string; direction: SyncDirection; comparison: SyncComparison; busy: boolean; executionBusy: boolean; error: string; exclusions: string; conflictChoices: Record<string, SyncConflictChoice>; progress: SyncExecutionProgress | null; result: SyncExecutionResult | null; history: SyncHistory[]; t: typeof copy[keyof typeof copy]; text: typeof syncUiCopy[keyof typeof syncUiCopy]; onClose: () => void; onDirection: (direction: SyncDirection) => void; onComparison: (comparison: SyncComparison) => void; onExclusions: (value: string) => void; onConflict: (path: string, choice: SyncConflictChoice) => void; onRefresh: () => void; onExecute: () => void; onCancelExecution: () => void; onClearHistory: () => void }) {
  const actionLabel = (action: SyncAction) => ({ upload: text.upload, download: text.download, createRemoteDirectory: text.createRemoteDirectory, createLocalDirectory: text.createLocalDirectory, conflict: text.conflict, destinationOnly: text.destinationOnlyAction })[action];
  const executableCount = preview?.items.filter((item) => item.action !== 'destinationOnly' && (item.action !== 'conflict' || (!item.isDirectory && conflictChoices[item.path] === 'source'))).length ?? 0;
  const statusLabel = (status: string) => status === 'Completed' ? text.completed : status === 'Cancelled' ? text.cancelled : text.failed;
  return <div className="modal-backdrop" role="presentation"><section className="connect-sheet sync-sheet">
    <div className="sheet-title"><div><h2>{text.title}</h2><p>{text.detail}</p></div><button type="button" disabled={executionBusy} onClick={onClose}>×</button></div>
    <div className="sync-paths"><span><strong>Local</strong>{localDirectory}</span><span><strong>Remote</strong>{remoteDirectory}</span></div>
    <div className="sync-controls"><label>{text.direction}<select value={direction} disabled={executionBusy} onChange={(event) => onDirection(event.target.value as SyncDirection)}><option value="localToRemote">{text.localToRemote}</option><option value="remoteToLocal">{text.remoteToLocal}</option></select></label><label>{text.comparison}<select value={comparison} disabled={executionBusy} onChange={(event) => onComparison(event.target.value as SyncComparison)}><option value="sizeOnly">{text.sizeOnly}</option><option value="sizeAndModified">{text.sizeAndModified}</option></select></label><button onClick={onRefresh} disabled={busy || executionBusy}>{busy && <LoaderCircle className="spinning" size={15}/>} {text.refresh}</button></div>
    <label className="sync-exclusions">{text.exclusions}<textarea value={exclusions} disabled={executionBusy} onChange={(event) => onExclusions(event.target.value)} placeholder={text.exclusionHint}/><small>{text.exclusionHint}</small></label>
    {error && <p className="form-error">{error}</p>}
    {preview && <><div className="sync-summary"><span>{text.transfers}<strong>{preview.transferCount}</strong></span><span>{text.directories}<strong>{preview.directoryCount}</strong></span><span className={preview.conflictCount ? 'warning' : ''}>{text.conflicts}<strong>{preview.conflictCount}</strong></span><span>{text.destinationOnly}<strong>{preview.destinationOnlyCount}</strong></span></div>
      <div className="sync-preview-list">{preview.items.length === 0 ? <p className="muted">{text.noChanges}</p> : preview.items.map((item) => <div className={`sync-preview-row ${item.action}`} key={`${item.action}-${item.path}`}><span>{actionLabel(item.action)}</span><strong>{item.path}</strong><small>{item.isDirectory ? '—' : `${formatBytes(item.localSize ?? 0)} / ${formatBytes(item.remoteSize ?? 0)}`}</small>{item.action === 'conflict' && <select value={conflictChoices[item.path] ?? 'skip'} disabled={executionBusy || item.isDirectory} onChange={(event) => onConflict(item.path, event.target.value as SyncConflictChoice)}><option value="skip">{text.skip}</option>{!item.isDirectory && <option value="source">{text.useSource}</option>}</select>}</div>)}</div></>}
    {executionBusy && progress && <div className="sync-execution-progress"><div><strong>{text.executing}</strong><span>{progress.completedItems} / {progress.totalItems}</span></div><progress value={progress.completedItems} max={Math.max(progress.totalItems, 1)}/><small>{progress.currentPath}</small></div>}
    {result && <section className={`sync-result ${result.status.toLowerCase()}`}><div><strong>{text.result}: {statusLabel(result.status)}</strong><span>{result.completedItems} / {result.totalItems} · {formatBytes(result.bytes)}</span></div>{result.log.filter((item) => item.status !== 'Completed').map((item) => <p key={`${item.action}-${item.path}`}><strong>{item.path}</strong>{item.detail}</p>)}</section>}
    <section className="sync-history"><div className="sync-history-heading"><strong>{text.history}</strong>{history.length > 0 && <button type="button" className="icon-button" aria-label={text.clearHistory} title={text.clearHistory} onClick={onClearHistory}><Trash2 size={14}/></button>}</div>{history.length === 0 ? <p className="muted">{text.noHistory}</p> : history.slice(0, 5).map((item) => <div className="sync-history-row" key={item.id}><span className={item.status.toLowerCase()}>{statusLabel(item.status)}</span><strong>{item.direction === 'localToRemote' ? text.localToRemote : text.remoteToLocal}</strong><small>{item.completedItems}/{item.totalItems} · {item.completedAt.slice(0, 16)}</small></div>)}</section>
    <div className="form-actions">{executionBusy ? <button type="button" onClick={onCancelExecution}>{text.cancelExecution}</button> : <><button type="button" onClick={onClose}>{t.cancel}</button><button type="button" className="primary" disabled={!preview || executableCount === 0 || busy} onClick={onExecute}>{text.execute} ({executableCount})</button></>}</div>
  </section></div>;
}

function ConnectSheet({ mode, bookmark, initialKeyPath, defaultProtocol, googleClientId, googleExportFormats, t, phaseCopy, passphraseText, localCopy, s3Text, sambaText, googleText, cloudFtpText, transferText, onClose, onSaved, onConnected }: { mode: 'connect' | 'edit'; bookmark: Connection | null; initialKeyPath: string; defaultProtocol: Protocol; googleClientId: string; googleExportFormats: GoogleExportPreferences; t: typeof copy[keyof typeof copy]; phaseCopy: typeof phaseOneCopy[keyof typeof phaseOneCopy]; passphraseText: typeof sshPassphrasePromptCopy[keyof typeof sshPassphrasePromptCopy]; localCopy: typeof bookmarkLocalCopy[keyof typeof bookmarkLocalCopy]; s3Text: typeof s3Copy[keyof typeof s3Copy]; sambaText: typeof sambaCopy[keyof typeof sambaCopy]; googleText: typeof googleDriveCopy[keyof typeof googleDriveCopy]; cloudFtpText: typeof cloudFtpCopy[keyof typeof cloudFtpCopy]; transferText: typeof transferSettingsCopy[keyof typeof transferSettingsCopy]; onClose: () => void; onSaved: (connection: Connection) => void; onConnected: (connection: Connection) => void }) {
  const [bookmarkName, setBookmarkName] = useState(bookmark?.name ?? '');
  const [protocol, setProtocol] = useState<Protocol>(bookmark?.protocol ?? defaultProtocol);
  const [host, setHost] = useState(bookmark?.host ?? '');
  const [port, setPort] = useState(bookmark?.port ?? defaultPort(defaultProtocol));
  const [initialDirectory, setInitialDirectory] = useState(bookmark?.initialPath ?? '/');
  const [username, setUsername] = useState(bookmark?.username ?? '');
  const [password, setPassword] = useState('');
  const [s3SessionToken, setS3SessionToken] = useState('');
  const [s3Region, setS3Region] = useState(bookmark?.s3Region ?? 'ap-northeast-1');
  const [s3Endpoint, setS3Endpoint] = useState(bookmark?.s3Endpoint ?? '');
  const [s3ForcePathStyle, setS3ForcePathStyle] = useState(bookmark?.s3ForcePathStyle ?? false);
  const [s3PreserveEmptyDirectories, setS3PreserveEmptyDirectories] = useState(bookmark?.s3PreserveEmptyDirectories ?? false);
  const [smbShare, setSmbShare] = useState(bookmark?.smbShare ?? '');
  const [smbDomain, setSmbDomain] = useState(bookmark?.smbDomain ?? '');
  const [smbGuest, setSmbGuest] = useState(bookmark?.smbGuest ?? false);
  const [googleDriveLocationKind, setGoogleDriveLocationKind] = useState<GoogleDriveLocationKind>(bookmark?.googleDriveLocationKind ?? 'myDrive');
  const [googleDriveLocationId, setGoogleDriveLocationId] = useState(bookmark?.googleDriveLocationId ?? '');
  const [googleDriveLocations, setGoogleDriveLocations] = useState<GoogleDriveLocation[]>([]);
  const [googleDriveLocationsLoading, setGoogleDriveLocationsLoading] = useState(false);
  const [keyPath, setKeyPath] = useState(initialKeyPath || bookmark?.keyPath || '');
  const [keyPassphraseNotRequired, setKeyPassphraseNotRequired] = useState(bookmark?.keyPassphraseNotRequired ?? false);
  const [saveKeyPassphrase, setSaveKeyPassphrase] = useState(true);
  const [tags, setTags] = useState(bookmark?.tags ?? '');
  const [localDirectory, setLocalDirectory] = useState(bookmark?.localDirectory ?? '');
  const [transferMaxConcurrent, setTransferMaxConcurrent] = useState<number | undefined>(bookmark?.transferMaxConcurrent);
  const [transferBandwidthLimitKbps, setTransferBandwidthLimitKbps] = useState<number | undefined>(bookmark?.transferBandwidthLimitKbps);
  const [transferRetryCount, setTransferRetryCount] = useState<number | undefined>(bookmark?.transferRetryCount);
  const [busy, setBusy] = useState(false);
  const [busyMessage, setBusyMessage] = useState('');
  const [error, setError] = useState('');
  const [showPassphrasePrompt, setShowPassphrasePrompt] = useState(false);
  const passphrasePromptOverride = useRef(false);
  function updateProtocol(value: Protocol) { setProtocol(value); setPort(defaultPort(value)); }
  async function selectLocalDirectory() {
    const selected = await open({ multiple: false, directory: true });
    if (selected && !Array.isArray(selected)) setLocalDirectory(selected);
  }
  async function selectSshKey() {
    const selected = await open({ multiple: false, directory: false });
    if (selected && !Array.isArray(selected)) setKeyPath(selected);
  }
  useEffect(() => { if (bookmark && bookmark.protocol !== 'googleDrive') void invoke<string | null>('credential_load', { bookmarkId: bookmark.id }).then((saved) => { if (!saved) return; if (bookmark.protocol === 's3') { try { const value = JSON.parse(saved) as { accessKeyId?: string; secretAccessKey?: string; sessionToken?: string }; setUsername(value.accessKeyId ?? ''); setPassword(value.secretAccessKey ?? ''); setS3SessionToken(value.sessionToken ?? ''); } catch { setUsername(''); setPassword(''); } } else if (!(bookmark.protocol === 'smb' && bookmark.smbGuest) && !bookmark.keyPassphraseNotRequired) setPassword(saved); }).catch((reason) => setError(`${passphraseText.keychainLoadFailed} ${invokeErrorMessage(reason)}`)); }, [bookmark, passphraseText.keychainLoadFailed]);
  useEffect(() => {
    if (protocol !== 'googleDrive' || !googleClientId.trim().endsWith('.apps.googleusercontent.com')) return;
    let active = true;
    setGoogleDriveLocationsLoading(true);
    void invoke<GoogleDriveLocation[]>('google_drive_locations', { clientId: googleClientId.trim() })
      .then((locations) => { if (active) setGoogleDriveLocations(locations); })
      .catch(() => { if (active) setGoogleDriveLocations([]); })
      .finally(() => { if (active) setGoogleDriveLocationsLoading(false); });
    return () => { active = false; };
  }, [protocol, googleClientId]);
  async function submit(event: React.FormEvent) {
    event.preventDefault(); setError('');
    const submitter = (event.nativeEvent as SubmitEvent).submitter as HTMLButtonElement | null;
    const intent: 'connect' | 'save' = submitter?.value === 'save' ? 'save' : 'connect';
    if (!Number.isInteger(port) || port < 1 || port > 65535) { setError(t.invalidPort); return; }
    if (protocol === 'cloudFtp' && !keyPath.trim()) { setError(cloudFtpText.keyRequired); return; }
    if (protocol === 'smb' && !smbShare.trim()) { setError(`${sambaText.share}: ${sambaText.shareHint}`); return; }
    if (protocol === 'googleDrive' && !googleClientId.trim().endsWith('.apps.googleusercontent.com')) { setError(googleText.invalidClientId); return; }
    setBusy(true); setBusyMessage(intent === 'connect' ? (isSshProtocol(protocol) ? t.checkingHostKey : t.connectingToServer) : '');
    try {
      const useNoKeyPassphrase = isSshProtocol(protocol) && Boolean(keyPath.trim()) && keyPassphraseNotRequired && !passphrasePromptOverride.current;
      const usesSshKeyPassphrase = isSshProtocol(protocol) && Boolean(keyPath.trim()) && !useNoKeyPassphrase;
      let resolvedPassword = password;
      if (usesSshKeyPassphrase && !resolvedPassword && saveKeyPassphrase && bookmark) {
        resolvedPassword = await invoke<string | null>('credential_load', { bookmarkId: bookmark.id }) ?? '';
        if (resolvedPassword) setPassword(resolvedPassword);
      }
      if (intent === 'connect' && usesSshKeyPassphrase && !resolvedPassword) {
        setError(passphraseText.keychainEntryMissing);
        setShowPassphrasePrompt(true);
        return;
      }
      const endpointUnchanged = bookmark?.protocol === protocol && bookmark.host === host && bookmark.port === port;
      let hostKey = endpointUnchanged ? bookmark?.hostKey : undefined;
      if (intent === 'connect') {
        hostKey = isSshProtocol(protocol) ? await invoke<string>('sftp_probe_host_key', { host, port }) : undefined;
        if (bookmark?.hostKey && endpointUnchanged && bookmark.hostKey !== hostKey) { setError(t.hostKeyChanged); return; }
        if (hostKey && (!bookmark?.hostKey || !endpointUnchanged) && !window.confirm(t.trustHostKey.replace('{{fingerprint}}', hostKey))) return;
      }
      let googleStatus: GoogleAuthorizationStatus | null = null;
      if (protocol === 'googleDrive') {
        googleStatus = await invoke<GoogleAuthorizationStatus>('google_drive_authorization_status', { clientId: googleClientId.trim() });
        if (intent === 'connect' && (!googleStatus.authorized || !googleStatus.clientMatches)) { setError(googleStatus.authorized ? googleText.mismatch : googleText.connectHint); return; }
      }
      const connectionHost = protocol === 'googleDrive' ? 'drive.google.com' : host.trim();
      const connectionUsername = protocol === 'googleDrive' ? (googleStatus?.email ?? 'Google Account') : protocol === 's3' || (protocol === 'smb' && smbGuest) ? '' : username;
      const connection: Connection = { id: bookmark?.id ?? crypto.randomUUID(), name: bookmarkName.trim() || (protocol === 'googleDrive' ? 'Google Drive' : connectionHost), protocol, host: connectionHost, port, username: connectionUsername, initialPath: initialDirectory.trim() || '/', keyPath: keyPath || undefined, keyPassphraseNotRequired: useNoKeyPassphrase, hostKey, localDirectory: localDirectory || undefined, tags, s3Region: protocol === 's3' ? s3Region.trim() : undefined, s3Endpoint: protocol === 's3' && s3Endpoint.trim() ? s3Endpoint.trim() : undefined, s3ForcePathStyle: protocol === 's3' && s3ForcePathStyle, s3PreserveEmptyDirectories: protocol === 's3' && s3PreserveEmptyDirectories, smbShare: protocol === 'smb' ? smbShare.trim() : undefined, smbDomain: protocol === 'smb' && smbDomain.trim() ? smbDomain.trim() : undefined, smbGuest: protocol === 'smb' && smbGuest, googleDriveLocationKind: protocol === 'googleDrive' ? googleDriveLocationKind : undefined, googleDriveLocationId: protocol === 'googleDrive' && googleDriveLocationKind === 'sharedDrive' ? googleDriveLocationId : undefined, transferMaxConcurrent, transferBandwidthLimitKbps, transferRetryCount };
      const storedCredential = protocol === 's3' ? JSON.stringify({ accessKeyId: username, secretAccessKey: resolvedPassword, sessionToken: s3SessionToken || undefined }) : resolvedPassword;
      const shouldStoreCredential = protocol === 'googleDrive' || protocol === 'smb' && smbGuest ? false : !(isSshProtocol(protocol) && keyPath.trim()) || saveKeyPassphrase;
      const shouldDeleteStoredCredential = Boolean(bookmark && bookmark.protocol !== 'googleDrive' && (connection.keyPassphraseNotRequired || !shouldStoreCredential));
      if (intent === 'save') {
        if (shouldDeleteStoredCredential) await invoke('credential_delete', { bookmarkId: connection.id });
        else if (resolvedPassword) await invoke('credential_save', { bookmarkId: connection.id, password: storedCredential });
        await invoke('bookmark_save', { bookmark: connection });
        onSaved(connection);
        return;
      }
      setBusyMessage(t.connectingToServer);
      await invoke('connection_connect', { request: { connectionId: connection.id, protocol, host: connection.host, port, username: connection.username, password: protocol === 'googleDrive' || isSshProtocol(protocol) && keyPath.trim() || (protocol === 'smb' && smbGuest) ? null : resolvedPassword || null, keyPath: keyPath.trim() || null, passphrase: usesSshKeyPassphrase ? resolvedPassword : null, expectedHostKey: hostKey ?? null, initialPath: connection.initialPath, s3Region: connection.s3Region ?? null, s3Endpoint: connection.s3Endpoint ?? null, s3SessionToken: s3SessionToken || null, s3ForcePathStyle: connection.s3ForcePathStyle ?? false, s3PreserveEmptyDirectories: connection.s3PreserveEmptyDirectories ?? false, smbShare: connection.smbShare ?? null, smbDomain: connection.smbDomain ?? null, smbGuest: connection.smbGuest ?? false, googleClientId: protocol === 'googleDrive' ? googleClientId.trim() : null, googleDriveLocationKind: connection.googleDriveLocationKind ?? null, googleDriveLocationId: connection.googleDriveLocationId ?? null, googleDocsExport: googleExportFormats.googleDocsExport, googleSheetsExport: googleExportFormats.googleSheetsExport, googleSlidesExport: googleExportFormats.googleSlidesExport, googleDrawingsExport: googleExportFormats.googleDrawingsExport } });
      if (shouldDeleteStoredCredential) await invoke('credential_delete', { bookmarkId: connection.id });
      else if (resolvedPassword) await invoke('credential_save', { bookmarkId: connection.id, password: storedCredential });
      onConnected(connection);
    } catch (reason) {
      const message = invokeErrorMessage(reason);
      setError(message);
      if (isSshProtocol(protocol) && keyPath.trim() && keyPassphraseNotRequired && /(passphrase|encrypted|decrypt)/i.test(message)) {
        setPassword('');
        setShowPassphrasePrompt(true);
      }
    }
    finally { setBusy(false); setBusyMessage(''); }
  }
  return <div className="modal-backdrop" role="presentation"><form className="connect-sheet bookmark-sheet" onSubmit={submit}>
    <div className="sheet-title"><div><h2>{mode === 'edit' ? t.editBookmarkTitle : t.connectTitle}</h2><p>{mode === 'edit' ? t.editBookmark : t.connect}</p></div><button type="button" onClick={onClose}>×</button></div>
    <div className="bookmark-sheet-scroll">
      <label>{t.bookmarkName}<input {...technicalInputProps} required value={bookmarkName} onChange={(event) => setBookmarkName(event.target.value)} placeholder={t.bookmarkNameHint}/></label>
      <label>{t.protocol}<select value={protocol} onChange={(event) => updateProtocol(event.target.value as Protocol)}><option value="sftp">SFTP</option><option value="cloudFtp">Google Cloud FTP (SFTP)</option><option value="ftp">FTP</option><option value="ftps">Explicit FTPS</option><option value="webdav">WebDAV (HTTPS)</option><option value="s3">Amazon S3 / S3-compatible</option><option value="smb">Samba / SMB 2/3</option><option value="googleDrive">Google Drive</option></select></label>
      {protocol !== 'googleDrive' && <div className="form-grid"><label>{protocol === 's3' ? s3Text.bucket : t.host}<input {...technicalInputProps} required value={host} onChange={(event) => setHost(event.target.value)} placeholder={protocol === 's3' ? 'example-bucket' : 'example.com'}/></label>{protocol !== 's3' && <label>{t.port}<input required min={1} max={65535} step={1} type="number" value={port} onChange={(event) => setPort(Number(event.target.value))}/></label>}</div>}
      {protocol === 'googleDrive' && <p className="protocol-security-hint google-drive-connect-hint"><Cloud size={17}/><span>{googleText.connectHint}</span></p>}
      {protocol === 'googleDrive' && <label>{googleText.location}<select value={`${googleDriveLocationKind}:${googleDriveLocationId}`} disabled={googleDriveLocationsLoading} onChange={(event) => { const [kind, ...id] = event.target.value.split(':'); setGoogleDriveLocationKind(kind as GoogleDriveLocationKind); setGoogleDriveLocationId(id.join(':')); setInitialDirectory('/'); }}><option value="myDrive:">{googleText.myDrive}</option><option value="sharedWithMe:">{googleText.sharedWithMe}</option>{googleDriveLocations.filter((location) => location.kind === 'sharedDrive').map((location) => <option key={location.id} value={`sharedDrive:${location.id ?? ''}`}>{location.name}</option>)}{googleDriveLocationKind === 'sharedDrive' && googleDriveLocationId && !googleDriveLocations.some((location) => location.id === googleDriveLocationId) && <option value={`sharedDrive:${googleDriveLocationId}`}>{bookmark?.name ?? googleDriveLocationId}</option>}</select><small className="field-hint">{googleDriveLocationsLoading ? googleText.loadingLocations : googleDriveLocations.length === 0 ? googleText.locationLoadFailed : ''}</small></label>}
      {protocol === 'cloudFtp' && <p className="protocol-security-hint google-drive-connect-hint"><Cloud size={17}/><span>{cloudFtpText.hint}</span></p>}
      {protocol === 'webdav' && <p className="protocol-security-hint">{phaseCopy.webdavHint}</p>}
      <label>{t.initialDirectory}<input {...technicalInputProps} required value={initialDirectory} onChange={(event) => setInitialDirectory(event.target.value)} placeholder={t.initialDirectoryHint}/></label>
      {protocol === 's3' && <><p className="protocol-security-hint">{s3Text.readOnly}</p><div className="form-grid"><label>{s3Text.region}<input {...technicalInputProps} required value={s3Region} onChange={(event) => setS3Region(event.target.value)} placeholder="ap-northeast-1"/></label><label>{s3Text.endpoint}<input {...technicalInputProps} type="url" value={s3Endpoint} onChange={(event) => setS3Endpoint(event.target.value)} placeholder="https://s3.example.com"/></label></div></>}
      {protocol === 'smb' && <><div className="form-grid"><label>{sambaText.share}<input {...technicalInputProps} required value={smbShare} onChange={(event) => setSmbShare(event.target.value)} placeholder={sambaText.shareHint}/></label><label>{sambaText.domain}<input {...technicalInputProps} value={smbDomain} onChange={(event) => setSmbDomain(event.target.value)} placeholder={sambaText.domainHint}/></label></div><label className="check-row"><input type="checkbox" checked={smbGuest} onChange={(event) => { setSmbGuest(event.target.checked); if (event.target.checked) setPassword(''); }}/><span>{sambaText.guest}</span></label><p className="protocol-security-hint">{sambaText.security}</p></>}
      {protocol !== 'googleDrive' && !(protocol === 'smb' && smbGuest) && <label>{protocol === 's3' ? s3Text.accessKey : t.user}<input {...technicalInputProps} required value={username} onChange={(event) => setUsername(event.target.value)}/></label>}{protocol !== 'googleDrive' && !(isSshProtocol(protocol) && keyPath.trim()) && protocol !== 'cloudFtp' && !(protocol === 'smb' && smbGuest) && <label>{protocol === 's3' ? s3Text.secretKey : t.password}<input {...technicalInputProps} type="password" value={password} onChange={(event) => setPassword(event.target.value)}/></label>}
      {protocol === 's3' && <><label>{s3Text.sessionToken}<input type="password" value={s3SessionToken} onChange={(event) => setS3SessionToken(event.target.value)}/></label><label className="check-row"><input type="checkbox" checked={s3ForcePathStyle} onChange={(event) => setS3ForcePathStyle(event.target.checked)}/><span>{s3Text.pathStyle}</span></label><label className="check-row"><input type="checkbox" checked={s3PreserveEmptyDirectories} onChange={(event) => setS3PreserveEmptyDirectories(event.target.checked)}/><span>{s3Text.preserveEmpty}</span></label></>}
      {isSshProtocol(protocol) && <label>{t.key}<span className="ssh-key-picker"><input {...technicalInputProps} required={protocol === 'cloudFtp'} value={keyPath} onChange={(event) => setKeyPath(event.target.value)} placeholder="~/.ssh/id_ed25519"/><button type="button" onClick={() => void selectSshKey()}>{t.chooseKey}</button></span><small className="field-hint">{protocol === 'cloudFtp' ? cloudFtpText.keyRequired : t.keyFormatHint}</small></label>}
      {isSshProtocol(protocol) && keyPath.trim() && <><label className="check-row"><input type="checkbox" checked={keyPassphraseNotRequired} onChange={(event) => { passphrasePromptOverride.current = false; setKeyPassphraseNotRequired(event.target.checked); if (event.target.checked) setPassword(''); }}/><span>{passphraseText.withoutPassphrase}</span></label><p className="protocol-security-hint ssh-passphrase-option-hint">{passphraseText.withoutPassphraseDetail}</p>{!keyPassphraseNotRequired && <><label>{t.keyPassphrase}<input type="password" value={password} onChange={(event) => setPassword(event.target.value)}/></label><label className="check-row"><input type="checkbox" checked={saveKeyPassphrase} onChange={(event) => setSaveKeyPassphrase(event.target.checked)}/><span>{passphraseText.saveInKeychain}</span></label><p className="protocol-security-hint ssh-passphrase-option-hint">{passphraseText.saveInKeychainDetail}</p></>}</>}
      <label>{phaseCopy.tags}<input {...technicalInputProps} value={tags} onChange={(event) => setTags(event.target.value)} placeholder={phaseCopy.tagHint}/></label>
      <section className="bookmark-local-directory-section bookmark-transfer-settings">
        <div className="bookmark-local-directory-copy"><strong>{transferText.title}</strong><p>{transferText.bookmarkDetail}</p></div>
        <div className="form-grid"><label>{transferText.concurrent}<input type="number" min="1" max="16" step="1" value={transferMaxConcurrent ?? ''} placeholder={transferText.inherit} onChange={(event) => setTransferMaxConcurrent(event.target.value === '' ? undefined : Math.min(16, Math.max(1, Math.round(Number(event.target.value)))))}/></label><label>{transferText.bandwidth}<input type="number" min="0" max="10485760" step="1" value={transferBandwidthLimitKbps ?? ''} placeholder={transferText.inherit} onChange={(event) => setTransferBandwidthLimitKbps(event.target.value === '' ? undefined : Math.min(10_485_760, Math.max(0, Math.round(Number(event.target.value)))))}/></label><label>{transferText.retries}<input type="number" min="0" max="10" step="1" value={transferRetryCount ?? ''} placeholder={transferText.inherit} onChange={(event) => setTransferRetryCount(event.target.value === '' ? undefined : Math.min(10, Math.max(0, Math.round(Number(event.target.value)))))}/></label></div>
      </section>
      <section className="bookmark-local-directory-section">
        <div className="bookmark-local-directory-copy"><strong>{localCopy.title}</strong><p>{localCopy.detail}</p></div>
        <div className="bookmark-local-directory-picker"><div className={`local-directory-path ${localDirectory ? '' : 'empty'}`} title={localDirectory || localCopy.none}><Folder size={16}/><span>{localDirectory || localCopy.none}</span></div><button type="button" onClick={() => void selectLocalDirectory()}>{localCopy.select}</button>{localDirectory && <button type="button" onClick={() => setLocalDirectory('')}>{localCopy.clear}</button>}</div>
      </section>
    </div>
    {(busyMessage || error) && <div className={`connection-feedback ${error ? 'is-error' : ''}`} role={error ? 'alert' : 'status'} aria-live={error ? 'assertive' : 'polite'}>{error ? <><strong>{t.connectionFailed}</strong><span>{error}</span></> : <span>{busyMessage}</span>}</div>}
    <div className="form-actions"><button type="button" onClick={onClose}>{t.cancel}</button><button type="submit" value="save" disabled={busy}>{t.saveOnly}</button><button type="submit" value="connect" className="primary" disabled={busy}>{busy && <LoaderCircle className="spinning" size={16}/>} {t.start}</button></div>
    {showPassphrasePrompt && <div className="modal-backdrop passphrase-prompt-backdrop" role="presentation"><section className="connect-sheet passphrase-prompt" role="dialog" aria-modal="true" aria-labelledby="ssh-passphrase-prompt-title">
      <div className="sheet-title"><div><h2 id="ssh-passphrase-prompt-title">{passphraseText.title}</h2><p>{passphraseText.detail}</p></div></div>
      <label>{t.keyPassphrase}<input autoFocus type="password" value={password} onChange={(event) => setPassword(event.target.value)}/></label>
      <div className="form-actions"><button type="button" onClick={() => { passphrasePromptOverride.current = false; setShowPassphrasePrompt(false); }}>{t.cancel}</button><button type="submit" className="primary" disabled={!password} onClick={() => { passphrasePromptOverride.current = true; setShowPassphrasePrompt(false); }}>{passphraseText.retry}</button></div>
    </section></div>}
  </form></div>;
}

function SoftwareUpdateSheet({ state, language, onClose, onInstall, onRestart }: { state: SoftwareUpdateState; language: Language; onClose: () => void; onInstall: () => void; onRestart: () => void }) {
  const text = softwareUpdateCopy[language];
  const progress = state.totalBytes ? Math.min(100, Math.round((state.downloadedBytes / state.totalBytes) * 100)) : undefined;
  return <div className="modal-backdrop" role="presentation"><section className="connect-sheet software-update-sheet" role="dialog" aria-modal="true" aria-labelledby="software-update-title">
    <div className="sheet-title"><div><h2 id="software-update-title">{text.title}</h2><p>{state.version ? text.available.replace('{{version}}', state.version) : text.detail}</p></div><button type="button" disabled={state.phase === 'downloading'} onClick={onClose}>×</button></div>
    <div className="software-update-content">
      <div className="software-update-version"><span>Harbor Transfer</span><strong>{state.currentVersion || '—'}{state.version ? ` → ${state.version}` : ''}</strong></div>
      {state.body && <section className="release-notes"><h3>{text.releaseNotes}</h3><p>{state.body}</p></section>}
      {state.phase === 'downloading' && <div className="update-progress" aria-live="polite"><div><span>{text.downloading}</span><strong>{progress === undefined ? formatBytes(state.downloadedBytes) : `${progress}%`}</strong></div>{state.totalBytes ? <progress value={state.downloadedBytes} max={state.totalBytes}/> : <progress/>}</div>}
      {state.phase === 'ready' && <p className="software-update-ready"><Check size={18}/>{text.ready}</p>}
      {state.phase === 'error' && <p className="form-error">{text.failed}<br/><small>{state.error}</small></p>}
    </div>
    <div className="form-actions"><button type="button" disabled={state.phase === 'downloading'} onClick={onClose}>{text.later}</button>{state.phase === 'ready' ? <button type="button" className="primary" onClick={onRestart}>{text.restart}</button> : <button type="button" className="primary" disabled={state.phase === 'downloading' || !state.version} onClick={onInstall}>{state.phase === 'downloading' ? <><LoaderCircle className="spinning" size={15}/>{text.downloading}</> : text.download}</button>}</div>
  </section></div>;
}

function PreferencesSheet({ value, language, t, softwareUpdate, onCheckUpdate, onShowUpdate, onClose, onSave }: { value: Preferences; language: Language; t: typeof copy[keyof typeof copy]; softwareUpdate: SoftwareUpdateState; onCheckUpdate: () => void; onShowUpdate: () => void; onClose: () => void; onSave: (preferences: Preferences) => void }) {
  const [draft, setDraft] = useState(value);
  const [googleStatus, setGoogleStatus] = useState<GoogleAuthorizationStatus | null>(null);
  const [googleBusy, setGoogleBusy] = useState(false);
  const [googleError, setGoogleError] = useState('');
  const text = preferencesCopy[language];
  const updateText = softwareUpdateCopy[language];
  const googleText = googleDriveCopy[language];
  const transferText = transferSettingsCopy[language];
  const tabs = [
    { id: 'general' as const, label: text.general },
    { id: 'appearance' as const, label: text.appearance },
    { id: 'transfers' as const, label: text.transfers },
    { id: 'editor' as const, label: text.editorTab },
    { id: 'googleDrive' as const, label: googleText.tab },
    { id: 'security' as const, label: text.security },
    { id: 'updates' as const, label: text.updates },
  ];
  const [activeTab, setActiveTab] = useState<(typeof tabs)[number]['id']>('general');
  const normalizedGoogleClientId = draft.googleClientId.trim();
  const validGoogleClientId = normalizedGoogleClientId.endsWith('.apps.googleusercontent.com');
  useEffect(() => {
    if (activeTab !== 'googleDrive') return;
    let cancelled = false;
    setGoogleError('');
    void invoke<GoogleAuthorizationStatus>('google_drive_authorization_status', { clientId: normalizedGoogleClientId }).then((status) => {
      if (!cancelled) setGoogleStatus(status);
    }).catch((reason) => {
      if (!cancelled) { setGoogleStatus(null); setGoogleError(invokeErrorMessage(reason)); }
    });
    return () => { cancelled = true; };
    // The Keychain is read only when the user opens this tab. Editing the ID
    // invalidates the visible match below without repeatedly querying Keychain.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeTab]);
  async function selectEditor() {
    const selected = await chooseEditorApplication();
    if (selected) setDraft((current) => ({ ...current, editorPath: selected }));
  }
  async function openGoogleSetupPage(page: 'developers' | 'project' | 'drive-api' | 'auth' | 'clients') {
    setGoogleError('');
    try { await invoke('google_drive_open_setup_page', { page }); }
    catch (reason) { setGoogleError(`${googleText.openFailed} ${invokeErrorMessage(reason)}`); }
  }
  async function authorizeGoogleDrive() {
    if (!validGoogleClientId) { setGoogleError(googleText.invalidClientId); return; }
    setGoogleBusy(true); setGoogleError('');
    try { setGoogleStatus(await invoke<GoogleAuthorizationStatus>('google_drive_authorize', { clientId: normalizedGoogleClientId })); }
    catch (reason) { setGoogleError(`${googleText.authFailed} ${invokeErrorMessage(reason)}`); }
    finally { setGoogleBusy(false); }
  }
  async function importGoogleCredentials() {
    const selected = await open({ multiple: false, directory: false, filters: [{ name: 'Google OAuth credentials', extensions: ['json'] }] });
    if (!selected || Array.isArray(selected)) return;
    setGoogleBusy(true); setGoogleError('');
    try {
      const clientId = await invoke<string>('google_drive_import_credentials', { path: selected });
      setDraft((current) => ({ ...current, googleClientId: clientId }));
      setGoogleStatus(await invoke<GoogleAuthorizationStatus>('google_drive_authorization_status', { clientId }));
    } catch (reason) { setGoogleError(invokeErrorMessage(reason)); }
    finally { setGoogleBusy(false); }
  }
  async function disconnectGoogleDrive() {
    if (!window.confirm(googleText.revokeConfirm)) return;
    setGoogleBusy(true); setGoogleError('');
    try { await invoke('google_drive_disconnect'); setGoogleStatus((current) => ({ authorized: false, clientMatches: false, credentialsReady: current?.credentialsReady ?? false })); }
    catch (reason) { setGoogleError(invokeErrorMessage(reason)); }
    finally { setGoogleBusy(false); }
  }
  return <div className="modal-backdrop" role="presentation"><form className="connect-sheet preferences-sheet" onSubmit={(event) => { event.preventDefault(); onSave(draft); }}>
    <div className="sheet-title"><div><h2>{text.title}</h2><p>{text.detail}</p></div><button type="button" onClick={onClose}>×</button></div>
    <div className="preferences-layout">
      <nav className="preferences-tabs" role="tablist" aria-orientation="vertical" aria-label={text.title}>
        {tabs.map((tab) => <button key={tab.id} type="button" role="tab" aria-selected={activeTab === tab.id} aria-controls={`preferences-panel-${tab.id}`} tabIndex={activeTab === tab.id ? 0 : -1} className={activeTab === tab.id ? 'active' : ''} onClick={() => setActiveTab(tab.id)}>{tab.label}</button>)}
      </nav>
      <div className="preferences-sheet-scroll">
        {activeTab === 'general' && <section className="preferences-panel" id="preferences-panel-general" role="tabpanel"><h3>{text.general}</h3>
          <label>{text.language}<select value={draft.language} onChange={(event) => setDraft((current) => ({ ...current, language: event.target.value as Language }))}><option value="ja">日本語</option><option value="en">English</option><option value="zh-CN">简体中文</option></select></label>
          <label>{text.defaultProtocol}<select value={draft.defaultProtocol} onChange={(event) => setDraft((current) => ({ ...current, defaultProtocol: event.target.value as Protocol }))}><option value="sftp">SFTP</option><option value="cloudFtp">Google Cloud FTP (SFTP)</option><option value="ftp">FTP</option><option value="ftps">Explicit FTPS</option><option value="webdav">WebDAV (HTTPS)</option><option value="s3">Amazon S3 / S3-compatible</option><option value="smb">Samba / SMB 2/3</option><option value="googleDrive">Google Drive</option></select></label>
        </section>}
        {activeTab === 'appearance' && <section className="preferences-panel" id="preferences-panel-appearance" role="tabpanel"><h3>{text.appearance}</h3>
          <label>{text.theme}<select value={draft.theme} onChange={(event) => setDraft((current) => ({ ...current, theme: event.target.value as Preferences['theme'] }))}><option value="system">{text.system}</option><option value="light">{text.light}</option><option value="dark">{text.dark}</option></select></label>
          <label className="check-row"><input type="checkbox" checked={draft.showHiddenFiles} aria-describedby="show-hidden-files-detail" onChange={(event) => setDraft((current) => ({ ...current, showHiddenFiles: event.target.checked }))}/><span>{text.showHiddenFiles}</span></label>
          <p className="preferences-field-detail" id="show-hidden-files-detail">{text.showHiddenFilesDetail}</p>
          <label>{text.fileNameWeight}<select value={draft.fileNameFontWeight} onChange={(event) => setDraft((current) => ({ ...current, fileNameFontWeight: event.target.value as Preferences['fileNameFontWeight'] }))}><option value="normal">{text.normal}</option><option value="bold">{text.bold}</option></select></label>
          <label>{text.fileRowDensity}<select value={draft.fileRowDensity} onChange={(event) => setDraft((current) => ({ ...current, fileRowDensity: event.target.value as Preferences['fileRowDensity'] }))}><option value="extraCompact">{text.extraCompact}</option><option value="compact">{text.compact}</option><option value="standard">{text.standard}</option><option value="comfortable">{text.comfortable}</option></select></label>
          <div className="file-name-size-setting"><div className="preference-setting-heading"><strong>{text.fileNameSize}</strong><output>{draft.fileNameFontSize}px</output></div><p className="preferences-field-detail">{text.fileNameSizeDetail}</p><input aria-label={text.fileNameSize} type="range" min="10" max="20" step="1" value={draft.fileNameFontSize} onChange={(event) => setDraft((current) => ({ ...current, fileNameFontSize: Number(event.target.value) }))}/><div className="file-name-size-preview" style={{ fontSize: `${draft.fileNameFontSize}px`, fontWeight: draft.fileNameFontWeight === 'normal' ? 400 : 650 }}><FileText size={18}/><span>{text.fileNamePreview}</span></div></div>
        </section>}
        {activeTab === 'transfers' && <section className="preferences-panel" id="preferences-panel-transfers" role="tabpanel"><h3>{text.transfers}</h3>
          <p className="preferences-field-detail">{transferText.globalDetail}</p>
          <div className="form-grid"><label>{transferText.concurrent}<input type="number" min="1" max="16" step="1" value={draft.maxConcurrentTransfers} onChange={(event) => setDraft((current) => ({ ...current, maxConcurrentTransfers: Math.min(16, Math.max(1, Math.round(Number(event.target.value) || 1))) }))}/></label><label>{transferText.bandwidth}<input type="number" min="0" max="10485760" step="1" value={draft.bandwidthLimitKbps} onChange={(event) => setDraft((current) => ({ ...current, bandwidthLimitKbps: Math.min(10_485_760, Math.max(0, Math.round(Number(event.target.value) || 0))) }))}/></label><label>{transferText.retries}<input type="number" min="0" max="10" step="1" value={draft.automaticRetryCount} onChange={(event) => setDraft((current) => ({ ...current, automaticRetryCount: Math.min(10, Math.max(0, Math.round(Number(event.target.value) || 0))) }))}/></label></div>
          <label>{text.conflictPolicy}<select value={draft.conflictPolicy} onChange={(event) => setDraft((current) => ({ ...current, conflictPolicy: event.target.value as Preferences['conflictPolicy'] }))}><option value="ask">{text.ask}</option><option value="overwrite">{text.overwrite}</option><option value="skip">{text.skip}</option></select></label>
          <label className="check-row"><input type="checkbox" checked={draft.transferNotifications} onChange={(event) => setDraft((current) => ({ ...current, transferNotifications: event.target.checked }))}/><span>{text.notifications}</span></label>
        </section>}
        {activeTab === 'editor' && <section className="preferences-panel" id="preferences-panel-editor" role="tabpanel"><h3>{text.editor}</h3><p className="preferences-field-detail">{text.editorDetail}</p><div className="editor-picker"><input readOnly value={draft.editorPath} placeholder={text.noEditor}/><button type="button" onClick={() => void selectEditor()}>{text.chooseEditor}</button>{draft.editorPath && <button type="button" onClick={() => setDraft((current) => ({ ...current, editorPath: '' }))}>{text.clearEditor}</button>}</div></section>}
        {activeTab === 'googleDrive' && <section className="preferences-panel google-drive-preferences" id="preferences-panel-googleDrive" role="tabpanel"><div className="google-drive-heading"><span className="google-drive-mark"><Cloud size={22}/></span><div><h3>{googleText.setupTitle}</h3><p className="preferences-field-detail">{googleText.setupDetail}</p></div></div>
          <button className="google-developers-link" type="button" onClick={() => void openGoogleSetupPage('developers')}><Link2 size={15}/>{googleText.developers}</button>
          <ol className="google-setup-steps">
            <li><div><strong>{googleText.project}</strong><p>{googleText.stepProject}</p></div><button type="button" onClick={() => void openGoogleSetupPage('project')}><Link2 size={14}/></button></li>
            <li><div><strong>{googleText.api}</strong><p>{googleText.stepApi}</p></div><button type="button" onClick={() => void openGoogleSetupPage('drive-api')}><Link2 size={14}/></button></li>
            <li><div><strong>{googleText.consent}</strong><p>{googleText.stepConsent}</p></div><button type="button" onClick={() => void openGoogleSetupPage('auth')}><Link2 size={14}/></button></li>
            <li><div><strong>{googleText.client}</strong><p>{googleText.stepClient}</p></div><button type="button" onClick={() => void openGoogleSetupPage('clients')}><Link2 size={14}/></button></li>
          </ol>
          <div className="google-oauth-card"><p>{googleText.stepPaste}</p><button type="button" disabled={googleBusy} onClick={() => void importGoogleCredentials()}><FileJson size={15}/>{googleText.importCredentials}</button><label>{googleText.clientId}<input {...technicalInputProps} value={draft.googleClientId} onChange={(event) => { setDraft((current) => ({ ...current, googleClientId: event.target.value })); setGoogleStatus((current) => current ? { ...current, clientMatches: false, credentialsReady: false } : null); }} placeholder={googleText.clientIdHint} aria-invalid={Boolean(normalizedGoogleClientId) && !validGoogleClientId}/></label><p className={`google-credentials-status ${googleStatus?.credentialsReady ? 'ready' : ''}`}>{googleStatus?.credentialsReady ? googleText.credentialsReady : googleText.credentialsMissing}</p><p className="google-scope-warning">{googleText.scopeWarning}</p>
            <div className="google-auth-row"><span className={`google-auth-status ${googleStatus?.authorized && googleStatus.clientMatches ? 'connected' : ''}`}><span/>{googleStatus?.authorized ? (googleStatus.clientMatches ? `${googleText.connected}${googleStatus.email ? ` · ${googleStatus.email}` : ''}` : googleText.mismatch) : googleText.notConnected}</span><div>{googleStatus?.authorized && googleStatus.clientMatches && <button type="button" disabled={googleBusy} onClick={() => void disconnectGoogleDrive()}>{googleText.disconnect}</button>}<button type="button" className="primary" disabled={googleBusy || !validGoogleClientId || !googleStatus?.credentialsReady} onClick={() => void authorizeGoogleDrive()}>{googleBusy ? <><LoaderCircle className="spinning" size={14}/>{googleText.authorizing}</> : googleText.authorize}</button></div></div>
          </div>
          <section className="google-oauth-card google-export-settings"><h4>{googleText.exportTitle}</h4><p>{googleText.exportDetail}</p><div className="form-grid"><label>{googleText.documents}<select value={draft.googleDocsExport} onChange={(event) => setDraft((current) => ({ ...current, googleDocsExport: event.target.value as Preferences['googleDocsExport'] }))}><option value="docx">DOCX</option><option value="pdf">PDF</option><option value="odt">ODT</option><option value="txt">TXT</option></select></label><label>{googleText.spreadsheets}<select value={draft.googleSheetsExport} onChange={(event) => setDraft((current) => ({ ...current, googleSheetsExport: event.target.value as Preferences['googleSheetsExport'] }))}><option value="xlsx">XLSX</option><option value="pdf">PDF</option><option value="csv">CSV</option></select></label><label>{googleText.presentations}<select value={draft.googleSlidesExport} onChange={(event) => setDraft((current) => ({ ...current, googleSlidesExport: event.target.value as Preferences['googleSlidesExport'] }))}><option value="pptx">PPTX</option><option value="pdf">PDF</option></select></label><label>{googleText.drawings}<select value={draft.googleDrawingsExport} onChange={(event) => setDraft((current) => ({ ...current, googleDrawingsExport: event.target.value as Preferences['googleDrawingsExport'] }))}><option value="pdf">PDF</option><option value="png">PNG</option><option value="svg">SVG</option></select></label></div></section>
          {googleError && <p className="form-error" role="alert">{googleError}</p>}
        </section>}
        {activeTab === 'security' && <section className="preferences-panel" id="preferences-panel-security" role="tabpanel"><h3>{text.security}</h3><label className="check-row"><input type="checkbox" checked={draft.confirmDelete} onChange={(event) => setDraft((current) => ({ ...current, confirmDelete: event.target.checked }))}/><span>{text.confirmDelete}</span></label></section>}
        {activeTab === 'updates' && <section className="preferences-panel" id="preferences-panel-updates" role="tabpanel"><h3>{updateText.title}</h3><p className="preferences-field-detail">{updateText.detail}</p>
          <div className="software-update-summary"><span>{updateText.currentVersion}</span><strong>{softwareUpdate.currentVersion || '—'}</strong></div>
          <label className="check-row"><input type="checkbox" checked={draft.autoCheckUpdates} onChange={(event) => setDraft((current) => ({ ...current, autoCheckUpdates: event.target.checked }))}/><span>{updateText.automatic}</span></label>
          <div className="software-update-actions"><button type="button" disabled={softwareUpdate.phase === 'checking' || softwareUpdate.phase === 'downloading'} onClick={onCheckUpdate}>{softwareUpdate.phase === 'checking' ? <><LoaderCircle className="spinning" size={14}/>{updateText.checking}</> : updateText.check}</button>{(softwareUpdate.phase === 'available' || softwareUpdate.phase === 'downloading' || softwareUpdate.phase === 'ready') && <button type="button" className="primary" onClick={onShowUpdate}>{updateText.details}</button>}</div>
          {softwareUpdate.phase === 'current' && <p className="software-update-status success"><Check size={14}/>{updateText.current}</p>}
          {softwareUpdate.phase === 'available' && <p className="software-update-status">{updateText.available.replace('{{version}}', softwareUpdate.version ?? '')}</p>}
          {softwareUpdate.phase === 'error' && <p className="software-update-status error" title={softwareUpdate.error}>{updateText.failed}</p>}
        </section>}
      </div>
    </div>
    <div className="form-actions"><button type="button" onClick={onClose}>{t.cancel}</button><button className="primary">{text.save}</button></div>
  </form></div>;
}

function KeyManager({ t, text, onClose, onUse }: { t: typeof copy[keyof typeof copy]; text: typeof sshKeyCopy[keyof typeof sshKeyCopy]; onClose: () => void; onUse: (keyPath: string) => void }) {
  const [keys, setKeys] = useState<SshKey[]>([]);
  const [error, setError] = useState('');
  const [loaded, setLoaded] = useState(false);
  async function load() { setError(''); try { setKeys(await invoke<SshKey[]>('ssh_keys_list')); } catch (reason) { setError(String(reason)); } finally { setLoaded(true); } }
  useEffect(() => { void load(); }, []);
  return <section className="connect-sheet key-sheet standalone-key-sheet">
    <div className="sheet-title"><div><h2>{t.keyManager}</h2><p>{t.keyHint}</p></div><button type="button" onClick={onClose}>×</button></div>
    {error && <p className="form-error">{error}</p>}
    <div className="key-list">
      {loaded && keys.length === 0 && <p className="muted key-empty">{text.noKeys}</p>}
      {keys.map((key) => <div className={`key-row key-row-${key.keyType}`} key={key.path}>
        <span className="key-type-icon" aria-hidden="true">{key.keyType === 'private' ? <LockKeyhole size={19}/> : <Share2 size={19}/>}</span>
        <div><span className="key-name-line"><strong>{key.name}</strong><span className="key-type-badge">{key.keyType === 'private' ? text.privateKey : text.publicKey}</span></span><small>{key.kind}</small><small title={key.path}>{key.path}</small>{key.pairedKeyPath && <em>{key.keyType === 'private' ? text.pairedPublic : text.pairedPrivate}</em>}</div>
        {key.keyType === 'private' ? <button className="primary" onClick={() => onUse(key.path)}>{text.usePrivate}</button> : <span className="public-key-note">{text.publicInfo}</span>}
      </div>)}
    </div>
    <div className="form-actions"><button type="button" onClick={() => void load()}>{t.refresh}</button><button type="button" onClick={onClose}>{t.cancel}</button></div>
  </section>;
}

export function SshKeyManagerWindow() {
  const preferences = loadPreferences();
  const t = copy[preferences.language];
  const keyText = sshKeyCopy[preferences.language];
  async function closeWindow() { await getCurrentWebviewWindow().close(); }
  async function selectKey(keyPath: string) {
    const targetLabel = new URLSearchParams(window.location.search).get('target') ?? 'main';
    await emitTo(targetLabel, 'ssh-key://selected', keyPath);
    await closeWindow();
  }
  return <main className="key-manager-window"><KeyManager t={t} text={keyText} onClose={() => void closeWindow()} onUse={(keyPath) => void selectKey(keyPath)}/></main>;
}
