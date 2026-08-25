import { invoke } from '@tauri-apps/api/core';
import { startDrag } from '@crabnebula/tauri-plugin-drag';
import { open, save } from '@tauri-apps/plugin-dialog';
import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';
import { emitTo, listen } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { getCurrentWebviewWindow, WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { useEffect, useMemo, useRef, useState } from 'react';
import {
  ArrowUpToLine, Check, ChevronDown, ChevronLeft, ChevronRight, ChevronUp, Cloud, Columns3, Copy, File, Folder, FolderPlus, Grid2X2, HardDrive,
  FileDown, FileUp, FolderSync, FolderUp, KeyRound, List, LoaderCircle, MoreHorizontal, PanelLeftClose, PanelLeftOpen, Pencil, RefreshCw, Search, Settings, Trash2, Upload,
} from 'lucide-react';

type Protocol = 'sftp' | 'ftp' | 'ftps' | 'webdav' | 's3';
type FileEntry = { name: string; size: number; modified?: string; permissions?: string; file_type: 'File' | 'Directory' | 'Symlink' };
type Connection = { id: string; name: string; protocol: Protocol; host: string; port: number; username: string; initialPath: string; keyPath?: string; hostKey?: string; localDirectory?: string; tags: string; s3Region?: string; s3Endpoint?: string; s3ForcePathStyle?: boolean };
type ConnectionHistory = { bookmarkId: string; name: string; protocol: Protocol; host: string; port: number; username: string; connectedAt: string };
type Transfer = { id: string; name: string; direction: 'Upload' | 'Download'; status: 'Running' | 'Completed' | 'Failed' | 'Cancelled'; detail: string; localPath?: string; remotePath?: string; connectionId?: string; transferredBytes?: number; totalBytes?: number; speed?: number; etaSeconds?: number };
type SshKey = { name: string; path: string; publicKeyPath?: string; kind: string };
type DirectoryProgress = { transferId: string; completedFiles: number; totalFiles: number; currentPath: string; status: string };
type FileProgress = { transferId: string; transferredBytes: number; totalBytes: number; elapsedMs: number; status: string };
type LocalPathInfo = { name: string; isDirectory: boolean };
type TransferHistory = { id: string; name: string; direction: 'Upload' | 'Download'; status: 'Completed' | 'Failed' | 'Cancelled'; detail: string; bytes: number; completedAt: string };
type Language = 'ja' | 'en' | 'zh-CN';
type Preferences = { language: Language; theme: 'system' | 'light' | 'dark'; defaultProtocol: Protocol; conflictPolicy: 'ask' | 'overwrite' | 'skip'; confirmDelete: boolean; transferNotifications: boolean; editorPath: string };
type RemoteEdit = { editId: string; connectionId: string; name: string; remotePath: string; status: 'watching' | 'waiting' | 'failed'; detail?: string };
type RemoteEditOpenResult = { editId: string; name: string; remotePath: string };
type RemoteEditPollResult = { editId: string; remotePath: string; status: 'clean' | 'waiting' | 'uploaded'; bytes: number };
type DragExport = { exportId: string; name: string; remotePath: string; localPath: string; iconPath: string; connectionId: string };
type ColumnKey = 'name' | 'size' | 'modified' | 'permissions';
type ColumnWidths = Record<ColumnKey, number>;
type ViewMode = 'list' | 'icons' | 'columns';
type ColumnLevel = { path: string; entries: FileEntry[]; selectedName?: string };
type SyncDirection = 'localToRemote' | 'remoteToLocal';
type SyncAction = 'upload' | 'download' | 'createRemoteDirectory' | 'createLocalDirectory' | 'conflict' | 'destinationOnly';
type SyncPreviewItem = { path: string; action: SyncAction; localSize?: number; remoteSize?: number; isDirectory: boolean };
type SyncPreview = { direction: SyncDirection; items: SyncPreviewItem[]; transferCount: number; directoryCount: number; conflictCount: number; destinationOnlyCount: number };
type SyncConflictChoice = 'skip' | 'source';
type SyncExecutionLogItem = { path: string; action: SyncAction; status: string; detail: string; bytes: number };
type SyncExecutionResult = { syncId: string; status: string; completedItems: number; totalItems: number; bytes: number; log: SyncExecutionLogItem[] };
type SyncExecutionProgress = { syncId: string; completedItems: number; totalItems: number; currentPath: string; status: string };
type SyncHistory = { id: string; direction: SyncDirection; localDirectory: string; remoteDirectory: string; status: string; completedItems: number; totalItems: number; bytes: number; detail: string; completedAt: string };
type BookmarkExportFile = { format: 'harbor-transfer-bookmarks'; version: 1; exportedAt: string; bookmarks: Connection[] };

const defaultPreferences: Preferences = { language: 'ja', theme: 'system', defaultProtocol: 'sftp', conflictPolicy: 'ask', confirmDelete: true, transferNotifications: true, editorPath: '' };
function loadPreferences(): Preferences { try { return { ...defaultPreferences, ...JSON.parse(localStorage.getItem('harbor-transfer.preferences') ?? '{}') }; } catch { return defaultPreferences; } }
const minimumColumnWidths: ColumnWidths = { name: 160, size: 76, modified: 150, permissions: 104 };
const defaultColumnWidths: ColumnWidths = { name: 320, size: 76, modified: 150, permissions: 104 };
function loadColumnWidths(): ColumnWidths { try { return { ...defaultColumnWidths, ...JSON.parse(localStorage.getItem('harbor-transfer.column-widths-v2') ?? '{}') }; } catch { return defaultColumnWidths; } }

const copy = {
  ja: { title: 'Harbor Transfer', connect: '新規接続', bookmarks: 'ブックマーク', history: '履歴', transfer: '転送', refresh: '更新', upload: 'アップロード', uploadFolder: 'フォルダをアップロード', newFolder: '新規フォルダ', search: '検索', empty: '接続先を選択してください', emptyDetail: '新規接続を作成するか、ブックマークを選択して開始します。', path: 'パス', breadcrumbs: '現在のディレクトリ', copyPath: 'パスをコピー', pathCopied: 'パスをクリップボードにコピーしました', name: '名前', size: 'サイズ', modified: '更新日', status: '転送キュー', connectTitle: '新規接続', editBookmark: 'ブックマークを編集', editBookmarkTitle: 'ブックマークの編集', bookmarkSaved: 'ブックマークを更新しました', saveBookmark: '変更を保存', exportBookmarks: 'ブックマークを書き出す', importBookmarks: 'ブックマークを読み込む', bookmarksExported: 'ブックマークを書き出しました', bookmarksImported: '{{count}}件のブックマークを読み込みました', noBookmarksToExport: '書き出すブックマークがありません', invalidBookmarkFile: '有効なHarbor Transferブックマークファイルではありません', bookmarkName: 'ブックマーク名', bookmarkNameHint: '例：本番Webサーバー', initialDirectory: '接続時の初期ディレクトリ', initialDirectoryHint: '例：/var/www/html', cancel: 'キャンセル', pause: '停止', resume: '再開', retry: '再試行', start: '接続する', protocol: 'プロトコル', host: 'サーバー', port: 'ポート', user: 'ユーザー名', password: 'パスワード', key: 'SSH 鍵ファイル（任意）', keys: 'SSH キー', settings: '環境設定', keyManager: 'SSH キー・マネージャー', keyHint: '鍵の内容は読み込まず、ファイル情報だけを表示します。', useKey: 'この鍵を使用', noKeys: '~/.ssh に利用可能な秘密鍵がありません。', pairedKey: '公開鍵あり', hostKeyChanged: 'サーバーのホスト鍵が保存済みの鍵と一致しません。接続を中止しました。', trustHostKey: 'サーバーのホスト鍵を確認してください。\n\n{{fingerprint}}\n\nこのサーバーを信頼して接続しますか？', error: 'エラー', connected: '接続済み', download: 'ダウンロード' },
  en: { title: 'Harbor Transfer', connect: 'New Connection', bookmarks: 'Bookmarks', history: 'History', transfer: 'Transfers', refresh: 'Refresh', upload: 'Upload', uploadFolder: 'Upload Folder', newFolder: 'New Folder', search: 'Search', empty: 'Choose a connection', emptyDetail: 'Create a new connection or select a bookmark to get started.', path: 'Path', breadcrumbs: 'Current directory', copyPath: 'Copy path', pathCopied: 'Path copied to the clipboard', name: 'Name', size: 'Size', modified: 'Modified', status: 'Transfer Queue', connectTitle: 'New Connection', editBookmark: 'Edit bookmark', editBookmarkTitle: 'Edit Bookmark', bookmarkSaved: 'Bookmark updated', saveBookmark: 'Save Changes', exportBookmarks: 'Export bookmarks', importBookmarks: 'Import bookmarks', bookmarksExported: 'Bookmarks exported', bookmarksImported: 'Imported {{count}} bookmarks', noBookmarksToExport: 'There are no bookmarks to export', invalidBookmarkFile: 'This is not a valid Harbor Transfer bookmark file', bookmarkName: 'Bookmark name', bookmarkNameHint: 'e.g. Production Web Server', initialDirectory: 'Initial directory on connect', initialDirectoryHint: 'e.g. /var/www/html', cancel: 'Cancel', pause: 'Pause', resume: 'Resume', retry: 'Retry', start: 'Connect', protocol: 'Protocol', host: 'Server', port: 'Port', user: 'Username', password: 'Password', key: 'SSH key file (optional)', keys: 'SSH Keys', settings: 'Preferences', keyManager: 'SSH Key Manager', keyHint: 'Key contents are never read; only file metadata is shown.', useKey: 'Use this key', noKeys: 'No private keys are available in ~/.ssh.', pairedKey: 'Public key found', hostKeyChanged: 'The server host key differs from the saved key. Connection was stopped.', trustHostKey: 'Verify the server host key.\n\n{{fingerprint}}\n\nTrust this server and connect?', error: 'Error', connected: 'Connected', download: 'Download' },
  'zh-CN': { title: 'Harbor Transfer', connect: '新建连接', bookmarks: '书签', history: '历史记录', transfer: '传输', refresh: '刷新', upload: '上传', uploadFolder: '上传文件夹', newFolder: '新建文件夹', search: '搜索', empty: '选择一个连接', emptyDetail: '创建新连接或选择书签以开始使用。', path: '路径', breadcrumbs: '当前目录', copyPath: '复制路径', pathCopied: '路径已复制到剪贴板', name: '名称', size: '大小', modified: '修改日期', status: '传输队列', connectTitle: '新建连接', editBookmark: '编辑书签', editBookmarkTitle: '编辑书签', bookmarkSaved: '书签已更新', saveBookmark: '保存更改', exportBookmarks: '导出书签', importBookmarks: '导入书签', bookmarksExported: '书签已导出', bookmarksImported: '已导入{{count}}个书签', noBookmarksToExport: '没有可导出的书签', invalidBookmarkFile: '这不是有效的Harbor Transfer书签文件', bookmarkName: '书签名称', bookmarkNameHint: '例如：生产环境 Web 服务器', initialDirectory: '连接时的初始目录', initialDirectoryHint: '例如：/var/www/html', cancel: '取消', pause: '暂停', resume: '继续', retry: '重试', start: '连接', protocol: '协议', host: '服务器', port: '端口', user: '用户名', password: '密码', key: 'SSH 密钥文件（可选）', keys: 'SSH 密钥', settings: '偏好设置', keyManager: 'SSH 密钥管理器', keyHint: '不会读取密钥内容，仅显示文件信息。', useKey: '使用此密钥', noKeys: '~/.ssh 中没有可用的私钥。', pairedKey: '已找到公钥', hostKeyChanged: '服务器主机密钥与已保存的密钥不一致，已停止连接。', trustHostKey: '请验证服务器主机密钥。\n\n{{fingerprint}}\n\n信任此服务器并连接吗？', error: '错误', connected: '已连接', download: '下载' },
} as const;

const phaseOneCopy = {
  ja: { tags: 'タグ', all: 'すべて', noHistory: '接続履歴はありません', tagHint: 'カンマ区切り（例: 本番, Web）', webdavHint: '証明書を検証するHTTPS接続だけを使用します。NextcloudではDAVのパスを初期ディレクトリに指定してください。' },
  en: { tags: 'Tags', all: 'All', noHistory: 'No connection history', tagHint: 'Comma separated (e.g. Production, Web)', webdavHint: 'Only HTTPS with certificate verification is used. For Nextcloud, enter the DAV path as the initial directory.' },
  'zh-CN': { tags: '标签', all: '全部', noHistory: '没有连接历史记录', tagHint: '使用逗号分隔（例如：生产, Web）', webdavHint: '仅使用经过证书验证的HTTPS连接。使用Nextcloud时，请将DAV路径填写为初始目录。' },
} as const;

const bookmarkLocalCopy = {
  ja: { title: 'ローカルディレクトリ', detail: '将来の差分同期で、この接続先と組み合わせるフォルダです。', select: 'フォルダを選択', clear: '解除', none: '選択されていません' },
  en: { title: 'Local directory', detail: 'Pair a folder with this connection for future differential sync.', select: 'Choose Folder', clear: 'Clear', none: 'Not selected' },
  'zh-CN': { title: '本地目录', detail: '为今后的差异同步将文件夹与此连接配对。', select: '选择文件夹', clear: '清除', none: '未选择' },
} as const;

const s3Copy = {
  ja: { bucket: 'バケット', accessKey: 'Access Key ID', secretKey: 'Secret Access Key', sessionToken: 'Session Token（任意）', region: 'リージョン', endpoint: 'カスタムエンドポイント（任意・HTTPS）', pathStyle: 'パス形式のURLを使用', readOnly: 'S3は単一ファイルのアップロード／ダウンロードに対応しています。フォルダ作成、編集、削除、同期はまだ無効です。' },
  en: { bucket: 'Bucket', accessKey: 'Access Key ID', secretKey: 'Secret Access Key', sessionToken: 'Session Token (optional)', region: 'Region', endpoint: 'Custom endpoint (optional, HTTPS)', pathStyle: 'Use path-style URLs', readOnly: 'S3 supports single-file uploads and downloads. Directory creation, editing, deletion, and sync remain disabled.' },
  'zh-CN': { bucket: '存储桶', accessKey: 'Access Key ID', secretKey: 'Secret Access Key', sessionToken: 'Session Token（可选）', region: '区域', endpoint: '自定义端点（可选，仅HTTPS）', pathStyle: '使用路径样式 URL', readOnly: 'S3支持单文件上传和下载。创建文件夹、编辑、删除和同步仍处于禁用状态。' },
} as const;

const phaseTwoCopy = {
  ja: { conflict: '同名の項目があります。「上書き」「スキップ」「別名」のいずれかを入力してください。', overwrite: '上書き', skip: 'スキップ', rename: '別名', action: '操作を入力してください: edit / download / rename / delete', renameTo: '新しい名前', deleteConfirm: 'この項目を削除しますか？', drop: 'ここにファイルやフォルダをドロップ', speed: '速度', eta: '残り', completed: '転送が完了しました', failed: '転送に失敗しました', cancelled: '転送を取り消しました' },
  en: { conflict: 'An item with this name exists. Enter overwrite, skip, or rename.', overwrite: 'overwrite', skip: 'skip', rename: 'rename', action: 'Enter action: edit / download / rename / delete', renameTo: 'New name', deleteConfirm: 'Delete this item?', drop: 'Drop files or folders here', speed: 'Speed', eta: 'ETA', completed: 'Transfer completed', failed: 'Transfer failed', cancelled: 'Transfer cancelled' },
  'zh-CN': { conflict: '存在同名项目。请输入 overwrite、skip 或 rename。', overwrite: 'overwrite', skip: 'skip', rename: 'rename', action: '输入操作：edit / download / rename / delete', renameTo: '新名称', deleteConfirm: '删除此项目吗？', drop: '将文件或文件夹拖放到这里', speed: '速度', eta: '剩余', completed: '传输完成', failed: '传输失败', cancelled: '传输已取消' },
} as const;

const preferencesCopy = {
  ja: { title: '環境設定', detail: 'すべての接続に適用する共通設定です。', general: '一般', appearance: '外観', theme: 'カラーテーマ', system: 'システム設定', light: 'ライト', dark: 'ダーク', transfers: '転送', security: '安全性', editor: 'リモートファイルエディタ', editorDetail: 'キャッシュを開くアプリケーションです。保存を検知すると、同名のリモートファイルを自動的に上書きします。', chooseEditor: 'エディタを選択', clearEditor: '解除', noEditor: '選択されていません', language: '表示言語', defaultProtocol: '新規接続の既定プロトコル', conflictPolicy: '同名ファイルの既定動作', ask: '毎回確認', overwrite: '上書き', skip: 'スキップ', confirmDelete: '削除前に確認する', notifications: '転送結果を画面内に通知する', save: '保存' },
  en: { title: 'Preferences', detail: 'These settings apply to every connection.', general: 'General', appearance: 'Appearance', theme: 'Color theme', system: 'System', light: 'Light', dark: 'Dark', transfers: 'Transfers', security: 'Safety', editor: 'Remote File Editor', editorDetail: 'This application opens cached copies. Saving automatically overwrites the file at the same remote path.', chooseEditor: 'Choose Editor', clearEditor: 'Clear', noEditor: 'Not selected', language: 'Display language', defaultProtocol: 'Default protocol for new connections', conflictPolicy: 'Default duplicate-file action', ask: 'Ask every time', overwrite: 'Overwrite', skip: 'Skip', confirmDelete: 'Confirm before deleting', notifications: 'Show in-app transfer notifications', save: 'Save' },
  'zh-CN': { title: '偏好设置', detail: '这些设置适用于所有连接。', general: '通用', appearance: '外观', theme: '颜色主题', system: '跟随系统', light: '浅色', dark: '深色', transfers: '传输', security: '安全性', editor: '远程文件编辑器', editorDetail: '此应用用于打开缓存副本。保存后会自动覆盖同一路径下的远程文件。', chooseEditor: '选择编辑器', clearEditor: '清除', noEditor: '未选择', language: '显示语言', defaultProtocol: '新连接的默认协议', conflictPolicy: '同名文件的默认操作', ask: '每次询问', overwrite: '覆盖', skip: '跳过', confirmDelete: '删除前确认', notifications: '在应用内显示传输结果通知', save: '保存' },
} as const;

const accessibilityCopy = {
  ja: { back: '戻る', forward: '進む', parent: '親フォルダ', more: 'その他の操作' },
  en: { back: 'Back', forward: 'Forward', parent: 'Parent folder', more: 'More actions' },
  'zh-CN': { back: '后退', forward: '前进', parent: '上级文件夹', more: '更多操作' },
} as const;

const remoteEditCopy = {
  ja: { edit: '編集', configure: '環境設定でリモートファイルエディタを選択してください。', opening: '編集用キャッシュを準備しています', watching: '保存を監視中', waiting: '変更の安定を待っています', uploaded: 'リモートファイルを上書き保存しました', stop: '編集を終了してキャッシュを削除' },
  en: { edit: 'Edit', configure: 'Choose a remote file editor in Preferences first.', opening: 'Preparing the editing cache', watching: 'Watching for saves', waiting: 'Waiting for changes to settle', uploaded: 'Remote file overwritten with saved changes', stop: 'Stop editing and delete cache' },
  'zh-CN': { edit: '编辑', configure: '请先在偏好设置中选择远程文件编辑器。', opening: '正在准备编辑缓存', watching: '正在监视保存操作', waiting: '正在等待更改稳定', uploaded: '已用保存的更改覆盖远程文件', stop: '结束编辑并删除缓存' },
} as const;

const dragOutCopy = {
  ja: { preparing: 'ドラッグ用ファイルを準備しています', ready: 'Finderへドラッグできます', retry: '準備中です。完了後にもう一度ドラッグしてください。', copied: 'Finderへファイルをコピーしました', cancelled: 'ファイルのドラッグを取り消しました' },
  en: { preparing: 'Preparing item for dragging', ready: 'Ready to drag to Finder', retry: 'The item is still being prepared. Drag it again when ready.', copied: 'Item copied to Finder', cancelled: 'Item drag cancelled' },
  'zh-CN': { preparing: '正在准备拖放文件', ready: '可以拖到访达', retry: '文件仍在准备中。完成后请再次拖动。', copied: '文件已复制到访达', cancelled: '已取消文件拖动' },
} as const;

const queueCopy = {
  ja: { collapse: '転送キューを折りたたむ', expand: '転送キューを展開', hideSidebar: 'サイドメニューを隠す', showSidebar: 'サイドメニューを表示', clearConnectionHistory: '接続履歴を削除', clearTransferHistory: '完了・失敗した転送履歴を削除', confirmConnection: '接続履歴をすべて削除しますか？', confirmTransfer: '完了・失敗・取消済みの転送履歴をすべて削除しますか？' },
  en: { collapse: 'Collapse transfer queue', expand: 'Expand transfer queue', hideSidebar: 'Hide sidebar', showSidebar: 'Show sidebar', clearConnectionHistory: 'Clear connection history', clearTransferHistory: 'Clear completed and failed transfers', confirmConnection: 'Clear all connection history?', confirmTransfer: 'Clear all completed, failed, and cancelled transfer history?' },
  'zh-CN': { collapse: '折叠传输队列', expand: '展开传输队列', hideSidebar: '隐藏侧边栏', showSidebar: '显示侧边栏', clearConnectionHistory: '清除连接历史记录', clearTransferHistory: '清除已完成和失败的传输', confirmConnection: '清除所有连接历史记录吗？', confirmTransfer: '清除所有已完成、失败和取消的传输历史记录吗？' },
} as const;

const columnCopy = {
  ja: { permissions: 'パーミッション', resize: '列幅を変更' },
  en: { permissions: 'Permissions', resize: 'Resize column' },
  'zh-CN': { permissions: '权限', resize: '调整列宽' },
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

function joinPath(base: string, name: string) { return base === '/' ? `/${name}` : `${base.replace(/\/$/, '')}/${name}`; }
function defaultPort(protocol: Protocol) { return protocol === 'sftp' ? 22 : protocol === 'webdav' || protocol === 's3' ? 443 : 21; }
function connectionTargetChanged(left: Connection, right: Connection) { return left.protocol !== right.protocol || left.host !== right.host || left.port !== right.port || left.username !== right.username || left.s3Region !== right.s3Region || left.s3Endpoint !== right.s3Endpoint || Boolean(left.s3ForcePathStyle) !== Boolean(right.s3ForcePathStyle); }
function parentPath(path: string) { const parts = path.split('/').filter(Boolean); parts.pop(); return `/${parts.join('/')}` || '/'; }
function formatBytes(bytes: number) { if (!bytes) return '—'; const units = ['B', 'KB', 'MB', 'GB']; const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), 3); return `${(bytes / 1024 ** index).toFixed(index ? 1 : 0)} ${units[index]}`; }
function formatDuration(seconds?: number) { if (seconds === undefined || !Number.isFinite(seconds)) return '—'; if (seconds < 60) return `${Math.ceil(seconds)}s`; return `${Math.floor(seconds / 60)}m ${Math.ceil(seconds % 60)}s`; }
function transferFailureStatus(reason: unknown): 'Failed' | 'Cancelled' { return String(reason).toLowerCase().includes('cancel') ? 'Cancelled' : 'Failed'; }

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
      if (!(protocol === 'sftp' || protocol === 'ftp' || protocol === 'ftps' || protocol === 'webdav' || protocol === 's3') || !Number.isInteger(port) || (port as number) < 1 || (port as number) > 65535) return null;
      if (!(bookmark.keyPath === undefined || (typeof bookmark.keyPath === 'string' && bookmark.keyPath.length <= 4096)) || !(bookmark.hostKey === undefined || (typeof bookmark.hostKey === 'string' && bookmark.hostKey.length <= 4096)) || !(bookmark.localDirectory === undefined || (typeof bookmark.localDirectory === 'string' && bookmark.localDirectory.length <= 4096))) return null;
      const id = (bookmark.id as string).trim();
      const host = (bookmark.host as string).trim();
      const username = (bookmark.username as string).trim();
      if (!id || !host || (protocol !== 's3' && !username)) return null;
      imported.set(id, {
        id,
        name: (bookmark.name as string).trim() || host,
        protocol,
        host,
        port: port as number,
        username,
        initialPath: (bookmark.initialPath as string).trim() || '/',
        keyPath: typeof bookmark.keyPath === 'string' && bookmark.keyPath ? bookmark.keyPath : undefined,
        hostKey: typeof bookmark.hostKey === 'string' && bookmark.hostKey ? bookmark.hostKey : undefined,
        localDirectory: typeof bookmark.localDirectory === 'string' && bookmark.localDirectory ? bookmark.localDirectory : undefined,
        tags: bookmark.tags as string,
        s3Region: typeof bookmark.s3Region === 'string' ? bookmark.s3Region : undefined,
        s3Endpoint: typeof bookmark.s3Endpoint === 'string' ? bookmark.s3Endpoint : undefined,
        s3ForcePathStyle: bookmark.s3ForcePathStyle === true,
      });
    }
    return [...imported.values()];
  } catch {
    return null;
  }
}

function ResizableColumnHeader({ label, column, width, resizeLabel, onStart, onAdjust }: { label: string; column: ColumnKey; width: number; resizeLabel: string; onStart: (event: React.PointerEvent, column: ColumnKey) => void; onAdjust: (column: ColumnKey, delta: number) => void }) {
  return <span className="column-heading"><span>{label}</span><span className="column-resizer" role="separator" aria-label={`${resizeLabel}: ${label}`} aria-orientation="vertical" aria-valuenow={width} tabIndex={0} onPointerDown={(event) => onStart(event, column)} onKeyDown={(event) => { if (event.key === 'ArrowLeft') { event.preventDefault(); onAdjust(column, -10); } if (event.key === 'ArrowRight') { event.preventDefault(); onAdjust(column, 10); } }}/></span>;
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
  const syncText = syncCopy[language];
  const editText = remoteEditCopy[language];
  const dragText = dragOutCopy[language];
  const [connections, setConnections] = useState<Connection[]>([]);
  const [history, setHistory] = useState<ConnectionHistory[]>([]);
  const [selectedTag, setSelectedTag] = useState('');
  const [active, setActive] = useState<Connection | null>(null);
  const [path, setPath] = useState('/');
  const [directoryHistory, setDirectoryHistory] = useState<string[]>([]);
  const [directoryHistoryIndex, setDirectoryHistoryIndex] = useState(-1);
  const [entries, setEntries] = useState<FileEntry[]>([]);
  const [query, setQuery] = useState('');
  const [showConnect, setShowConnect] = useState(false);
  const [showPreferences, setShowPreferences] = useState(false);
  const [selectedKeyPath, setSelectedKeyPath] = useState('');
  const [connectingBookmark, setConnectingBookmark] = useState<Connection | null>(null);
  const [connectSheetMode, setConnectSheetMode] = useState<'connect' | 'edit'>('connect');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [transfers, setTransfers] = useState<Transfer[]>([]);
  const [directoryProgress, setDirectoryProgress] = useState<DirectoryProgress | null>(null);
  const [directoryPaused, setDirectoryPaused] = useState(false);
  const [transferPanelCollapsed, setTransferPanelCollapsed] = useState(false);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(() => localStorage.getItem('harbor-transfer.sidebar-collapsed') === 'true');
  const [columnWidths, setColumnWidths] = useState<ColumnWidths>(loadColumnWidths);
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
  const [selectedRemoteFile, setSelectedRemoteFile] = useState<{ connectionId: string; remotePath: string } | null>(null);
  const [dragExport, setDragExport] = useState<DragExport | null>(null);
  const [dragPreparingPath, setDragPreparingPath] = useState('');
  const remoteEditPolling = useRef<Set<string>>(new Set());
  const dragPreparationSequence = useRef(0);
  const dragExportRef = useRef<DragExport | null>(null);
  const dragPreparingRef = useRef('');
  const dragSelectionTimer = useRef<number | null>(null);
  const browserZoneRef = useRef<HTMLElement | null>(null);
  const filteredEntries = useMemo(() => entries.filter((entry) => entry.name.toLowerCase().includes(query.toLowerCase())), [entries, query]);
  const breadcrumbs = useMemo(() => {
    const segments = path.split('/').filter(Boolean);
    return [{ label: '/', path: '/' }, ...segments.map((segment, index) => ({ label: segment, path: `/${segments.slice(0, index + 1).join('/')}` }))];
  }, [path]);

  useEffect(() => {
    void Promise.all([
      invoke<Connection[]>('bookmarks_list'),
      invoke<ConnectionHistory[]>('connection_history_list'),
      invoke<TransferHistory[]>('transfer_history_list'),
      invoke<SyncHistory[]>('sync_history_list'),
    ]).then(([saved, recent, transferHistory, savedSyncHistory]) => {
      setConnections(saved);
      setHistory(recent);
      setTransfers(transferHistory.map((item) => ({ ...item, totalBytes: item.bytes, transferredBytes: item.bytes })));
      setSyncHistory(savedSyncHistory);
    }).catch((reason) => setError(String(reason)));
  }, []);

  useEffect(() => {
    if (!notice) return;
    const timeout = window.setTimeout(() => setNotice(''), 4000);
    return () => window.clearTimeout(timeout);
  }, [notice]);

  useEffect(() => {
    localStorage.setItem('harbor-transfer.preferences', JSON.stringify(preferences));
    document.documentElement.dataset.theme = preferences.theme;
  }, [preferences]);

  useEffect(() => {
    const onShortcut = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey)) return;
      const target = event.target as HTMLElement | null;
      if (target?.matches('input, textarea, select, [contenteditable="true"]')) return;
      if (event.key === ',') { event.preventDefault(); setShowPreferences(true); }
      if (event.key.toLowerCase() === 'n') { event.preventDefault(); setConnectingBookmark(null); setConnectSheetMode('connect'); setShowConnect(true); }
      if (event.key.toLowerCase() === 'r' && active) { event.preventDefault(); void loadDirectory(active, path); }
    };
    window.addEventListener('keydown', onShortcut);
    return () => window.removeEventListener('keydown', onShortcut);
  }, [active, path]);

  useEffect(() => {
    localStorage.setItem('harbor-transfer.column-widths-v2', JSON.stringify(columnWidths));
  }, [columnWidths]);

  useEffect(() => {
    localStorage.setItem('harbor-transfer.view-mode', viewMode);
  }, [viewMode]);

  useEffect(() => {
    localStorage.setItem('harbor-transfer.sync-exclusions', syncExclusions);
  }, [syncExclusions]);

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
      } : item));
    }).then((dispose) => { unlisten = dispose; });
    return () => unlisten?.();
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    if (!active) return;
    void getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type === 'leave') setIsDragOver(false);
      else if (event.payload.type === 'drop') {
        setIsDragOver(false);
        if (event.payload.paths.length) void uploadDroppedPaths(event.payload.paths);
      } else setIsDragOver(true);
    }).then((dispose) => { unlisten = dispose; }).catch(() => undefined);
    return () => unlisten?.();
  }, [active, path, entries]);

  const availableTags = useMemo(() => Array.from(new Set(connections.flatMap((connection) => connection.tags.split(',').map((tag) => tag.trim()).filter(Boolean)))).sort(), [connections]);
  const visibleConnections = useMemo(() => selectedTag ? connections.filter((connection) => connection.tags.split(',').map((tag) => tag.trim()).includes(selectedTag)) : connections, [connections, selectedTag]);

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
    const selectedLevels = columnLevels.slice(0, levelIndex + 1).map((item, index) => index === levelIndex ? { ...item, selectedName: entry.name } : item);
    if (entry.file_type !== 'Directory') {
      setColumnLevels(selectedLevels);
      setPath(level.path);
      setEntries(level.entries);
      return;
    }
    const childPath = joinPath(level.path, entry.name);
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
    setColumnWidths((current) => ({ ...current, [column]: Math.max(minimumColumnWidths[column], current[column] + delta) }));
  }

  function startColumnResize(event: React.PointerEvent, column: ColumnKey) {
    event.preventDefault();
    event.stopPropagation();
    const startX = event.clientX;
    const startWidth = columnWidths[column];
    document.body.classList.add('resizing-columns');
    const move = (moveEvent: PointerEvent) => setColumnWidths((current) => ({ ...current, [column]: Math.max(minimumColumnWidths[column], startWidth + moveEvent.clientX - startX) }));
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

  async function copyCurrentPath() {
    try {
      if (navigator.clipboard?.writeText) await navigator.clipboard.writeText(path);
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
      const existing = await WebviewWindow.getByLabel('ssh-key-manager');
      if (existing) {
        await existing.show();
        await existing.setFocus();
        return;
      }
      const keyWindow = new WebviewWindow('ssh-key-manager', {
        url: 'index.html#ssh-keys',
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
      const saved = await invoke<Connection[]>('bookmarks_list');
      setConnections(saved);
      setSelectedTag('');
      setActive((current) => {
        if (!current) return current;
        const updated = saved.find((bookmark) => bookmark.id === current.id);
        if (!updated) return current;
        const targetChanged = connectionTargetChanged(current, updated);
        return targetChanged ? null : { ...current, name: updated.name, initialPath: updated.initialPath, keyPath: updated.keyPath, localDirectory: updated.localDirectory, tags: updated.tags };
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

  async function enqueueFile(localPath: string, name: string) {
    if (!active) return;
    const remoteName = resolveRemoteName(name);
    if (!remoteName) return;
    const transferId = crypto.randomUUID();
    const transfer: Transfer = { id: transferId, name: remoteName, direction: 'Upload', status: 'Running', detail: path, localPath, remotePath: joinPath(path, remoteName), connectionId: active.id, transferredBytes: 0 };
    setTransfers((current) => [transfer, ...current]);
    try {
      const bytes = await invoke<number>('transfer_upload', { request: { transferId, connectionId: active.id, localPath, remotePath: transfer.remotePath } });
      setTransfers((current) => current.map((item) => item.id === transfer.id ? { ...item, status: 'Completed' } : item));
      recordTransfer(transfer, 'Completed', transfer.detail, bytes);
    } catch (reason) {
      const status = transferFailureStatus(reason);
      setTransfers((current) => current.map((item) => item.id === transfer.id ? { ...item, status, detail: String(reason) } : item));
      recordTransfer(transfer, status, String(reason));
    }
  }

  async function enqueueDirectory(localDirectory: string, name: string) {
    if (!active) return;
    const remoteName = resolveRemoteName(name);
    if (!remoteName) return;
    const transferId = crypto.randomUUID();
    const transfer: Transfer = { id: transferId, name: remoteName, direction: 'Upload', status: 'Running', detail: path };
    setDirectoryProgress({ transferId, completedFiles: 0, totalFiles: 0, currentPath: remoteName, status: 'preparing' });
    setDirectoryPaused(false);
    setTransfers((current) => [transfer, ...current]);
    try {
      await invoke('transfer_upload_directory', { request: { transferId, connectionId: active.id, localDirectory, remoteDirectory: joinPath(path, remoteName) } });
      setTransfers((current) => current.map((item) => item.id === transfer.id ? { ...item, status: 'Completed' } : item));
      recordTransfer(transfer, 'Completed');
      void loadDirectory();
    } catch (reason) {
      const status = transferFailureStatus(reason);
      setTransfers((current) => current.map((item) => item.id === transfer.id ? { ...item, status, detail: String(reason) } : item));
      recordTransfer(transfer, status, String(reason));
    }
  }

  async function uploadDroppedPaths(paths: string[]) {
    for (const localPath of paths) {
      try {
        const info = await invoke<LocalPathInfo>('local_path_info', { path: localPath });
        if (info.isDirectory) await enqueueDirectory(localPath, info.name);
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

  async function calculateSyncPreview(localDirectory = syncLocalDirectory, direction = syncDirection, exclusions = syncExclusions) {
    if (!active || !localDirectory) return;
    setSyncPreviewBusy(true); setSyncPreviewError('');
    try {
      const preview = await invoke<SyncPreview>('sync_preview', { request: { connectionId: active.id, localDirectory, remoteDirectory: path, direction, exclusions: parsedSyncExclusions(exclusions) } });
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
      const result = await invoke<SyncExecutionResult>('sync_execute', { request: { syncId, connectionId: active.id, localDirectory: syncLocalDirectory, remoteDirectory: path, direction: syncDirection, exclusions: parsedSyncExclusions(), items } });
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
      setTransfers((current) => current.filter((item) => item.status === 'Running'));
    } catch (reason) { setError(String(reason)); }
  }

  async function retryTransfer(transfer: Transfer) {
    if (!transfer.connectionId || !transfer.localPath || !transfer.remotePath) return;
    setTransfers((current) => current.map((item) => item.id === transfer.id ? { ...item, status: 'Running' } : item));
    try {
      const bytes = await invoke<number>(transfer.direction === 'Upload' ? 'transfer_upload' : 'transfer_download', { request: { transferId: transfer.id, connectionId: transfer.connectionId, localPath: transfer.localPath, remotePath: transfer.remotePath } });
      setTransfers((current) => current.map((item) => item.id === transfer.id ? { ...item, status: 'Completed' } : item));
      recordTransfer(transfer, 'Completed', transfer.detail, bytes);
    } catch (reason) { const status = transferFailureStatus(reason); setTransfers((current) => current.map((item) => item.id === transfer.id ? { ...item, status, detail: String(reason) } : item)); recordTransfer(transfer, status, String(reason)); }
  }

  async function downloadFile(entry: FileEntry, basePath = path) {
    if (!active || entry.file_type === 'Directory') return;
    const localPath = await save({ defaultPath: entry.name });
    if (!localPath) return;
    const transfer: Transfer = { id: crypto.randomUUID(), name: entry.name, direction: 'Download', status: 'Running', detail: basePath, localPath, remotePath: joinPath(basePath, entry.name), connectionId: active.id, transferredBytes: 0, totalBytes: entry.size };
    setTransfers((current) => [transfer, ...current]);
    try {
      const bytes = await invoke<number>('transfer_download', { request: { transferId: transfer.id, connectionId: active.id, localPath, remotePath: transfer.remotePath } });
      setTransfers((current) => current.map((item) => item.id === transfer.id ? { ...item, status: 'Completed' } : item));
      recordTransfer(transfer, 'Completed', transfer.detail, bytes);
    } catch (reason) { const status = transferFailureStatus(reason); setTransfers((current) => current.map((item) => item.id === transfer.id ? { ...item, status, detail: String(reason) } : item)); recordTransfer(transfer, status, String(reason)); }
  }

  async function createDirectory() {
    if (!active) return;
    const name = window.prompt(t.newFolder);
    if (!name) return;
    try { await invoke('remote_create_directory', { request: { connectionId: active.id, path: joinPath(path, name) } }); await loadDirectory(); }
    catch (reason) { setError(String(reason)); }
  }

  async function editRemoteFile(entry: FileEntry, basePath = path) {
    if (!active || entry.file_type !== 'File') return;
    if (!preferences.editorPath) {
      setNotice(editText.configure);
      setShowPreferences(true);
      return;
    }
    const remotePath = joinPath(basePath, entry.name);
    if (remoteEdits.some((edit) => edit.connectionId === active.id && edit.remotePath === remotePath)) {
      setNotice(`${editText.watching}: ${entry.name}`);
      return;
    }
    setNotice(`${editText.opening}: ${entry.name}`);
    try {
      const session = await invoke<RemoteEditOpenResult>('remote_edit_open', { request: { connectionId: active.id, remotePath, editorPath: preferences.editorPath } });
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

  function scheduleRemoteDragPreparation(entry: FileEntry, basePath = path) {
    if (!active) return;
    if (dragSelectionTimer.current !== null) window.clearTimeout(dragSelectionTimer.current);
    setSelectedRemoteFile({ connectionId: active.id, remotePath: joinPath(basePath, entry.name) });
    // Preparing a directory downloads its entire tree while holding the remote
    // connection. Do that only after an actual drag gesture, never on selection.
    if (entry.file_type === 'Directory') {
      dragSelectionTimer.current = null;
      return;
    }
    dragSelectionTimer.current = window.setTimeout(() => {
      dragSelectionTimer.current = null;
      void prepareRemoteDrag(entry, basePath);
    }, 220);
  }

  function cancelScheduledDragPreparation() {
    if (dragSelectionTimer.current !== null) window.clearTimeout(dragSelectionTimer.current);
    dragSelectionTimer.current = null;
  }

  async function prepareRemoteDrag(entry: FileEntry, basePath = path) {
    if (!active) return;
    const remotePath = joinPath(basePath, entry.name);
    setSelectedRemoteFile({ connectionId: active.id, remotePath });
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
      const result = await invoke<Omit<DragExport, 'connectionId'>>('drag_export_prepare', { request: { connectionId: active.id, remotePath, isDirectory: entry.file_type === 'Directory' } });
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
    const remotePath = joinPath(basePath, entry.name);
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

  async function manageEntry(entry: FileEntry, basePath = path) {
    if (!active) return;
    const action = window.prompt(p2.action, entry.file_type === 'Directory' ? 'rename' : 'edit')?.toLowerCase();
    if (action === 'edit') { await editRemoteFile(entry, basePath); return; }
    if (action === 'download') { await downloadFile(entry, basePath); return; }
    if (action === 'rename') {
      const newName = window.prompt(p2.renameTo, entry.name);
      if (!newName || newName === entry.name) return;
      try {
        await invoke('remote_rename', { request: { connectionId: active.id, oldPath: joinPath(basePath, entry.name), newPath: joinPath(basePath, newName) } });
        await loadDirectory(active, basePath);
      } catch (reason) { setError(String(reason)); }
    }
    if (action === 'delete' && (!preferences.confirmDelete || window.confirm(p2.deleteConfirm))) {
      try {
        await invoke('remote_delete', { request: { connectionId: active.id, path: joinPath(basePath, entry.name), isDirectory: entry.file_type === 'Directory' } });
        await loadDirectory(active, basePath);
      } catch (reason) { setError(String(reason)); }
    }
  }

  return <main className={`app-shell ${transferPanelCollapsed ? 'queue-collapsed' : ''}`}>
    <header className="toolbar" onPointerDown={startWindowDrag}>
      <div className="brand"><Cloud size={21}/><span>{t.title}</span></div>
      <button className="icon-button" aria-label={sidebarCollapsed ? queueText.showSidebar : queueText.hideSidebar} title={sidebarCollapsed ? queueText.showSidebar : queueText.hideSidebar} aria-expanded={!sidebarCollapsed} onClick={() => setSidebarCollapsed((current) => !current)}>{sidebarCollapsed ? <PanelLeftOpen size={17}/> : <PanelLeftClose size={17}/>}</button>
      <button className="primary" onClick={() => { setConnectingBookmark(null); setSelectedKeyPath(''); setConnectSheetMode('connect'); setShowConnect(true); }}><span>+</span>{t.connect}</button>
      <button onClick={() => void openKeyManagerWindow()}><KeyRound size={16}/>{t.keys}</button>
      <button onClick={() => setShowPreferences(true)}><Settings size={16}/>{t.settings}</button>
      <div className="toolbar-spacer" />
      <label className="language"><span>Language</span><select value={language} onChange={(event) => setPreferences((current) => ({ ...current, language: event.target.value as Language }))}><option value="ja">日本語</option><option value="en">English</option><option value="zh-CN">简体中文</option></select></label>
    </header>
    <section className={`workspace ${sidebarCollapsed ? 'sidebar-collapsed' : ''}`}>
      <aside className="sidebar">
        <div className="sidebar-section-heading bookmarks-heading">
          <div className="sidebar-label">{t.bookmarks}</div>
          <button className="icon-button" aria-label={t.importBookmarks} title={t.importBookmarks} onClick={() => void importBookmarks()}><FileUp size={14}/></button>
          <button className="icon-button" aria-label={t.exportBookmarks} title={t.exportBookmarks} disabled={connections.length === 0} onClick={() => void exportBookmarks()}><FileDown size={14}/></button>
        </div>
        {availableTags.length > 0 && <div className="tag-filter"><button className={!selectedTag ? 'active' : ''} onClick={() => setSelectedTag('')}>{p1.all}</button>{availableTags.map((tag) => <button key={tag} className={selectedTag === tag ? 'active' : ''} onClick={() => setSelectedTag(tag)}>{tag}</button>)}</div>}
        {connections.length === 0 && <p className="muted">No saved connections</p>}
        {visibleConnections.map((connection) => <div className="bookmark-row" key={connection.id}>
          <button className={`bookmark bookmark-main ${active?.id === connection.id ? 'selected' : ''}`} onClick={() => { if (active?.id === connection.id) { void loadDirectory(connection, path); } else { setConnectingBookmark(connection); setSelectedKeyPath(connection.keyPath ?? ''); setConnectSheetMode('connect'); setShowConnect(true); } }}><HardDrive size={16}/><span>{connection.name}</span><small>{connection.protocol.toUpperCase()}</small></button>
          <button className="bookmark-edit-button" aria-label={`${t.editBookmark}: ${connection.name}`} title={t.editBookmark} onClick={() => { setConnectingBookmark(connection); setSelectedKeyPath(connection.keyPath ?? ''); setConnectSheetMode('edit'); setShowConnect(true); }}><Pencil size={14}/></button>
        </div>)}
        <div className="sidebar-section-heading history-label"><div className="sidebar-label">{t.history}</div>{history.length > 0 && <button className="icon-button" aria-label={queueText.clearConnectionHistory} title={queueText.clearConnectionHistory} onClick={() => void clearConnectionHistory()}><Trash2 size={14}/></button>}</div>
        {history.length === 0 ? <p className="muted">{p1.noHistory}</p> : history.slice(0, 6).map((item, index) => <button className="bookmark history-item" key={`${item.bookmarkId}-${item.connectedAt}-${index}`} onClick={() => { const saved = connections.find((connection) => connection.id === item.bookmarkId); const connection = saved ?? { id: item.bookmarkId, name: item.name, protocol: item.protocol, host: item.host, port: item.port, username: item.username, initialPath: '/', tags: '' }; setConnectingBookmark(connection); setSelectedKeyPath(connection.keyPath ?? ''); setConnectSheetMode('connect'); setShowConnect(true); }}><HardDrive size={14}/><span>{item.name}</span><small>{item.connectedAt.slice(5, 16)}</small></button>)}
      </aside>
      <section className={`browser ${isDragOver ? 'drag-over' : ''}`} ref={browserZoneRef}>
        {notice && <div className="notice-banner" role="status" aria-live="polite">{notice}</div>}
        {isDragOver && <div className="drop-overlay"><Upload size={32}/><strong>{p2.drop}</strong></div>}
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
            <button disabled={active.protocol === 's3'} title={active.protocol === 's3' ? s3Copy[language].readOnly : undefined} onClick={() => void openSyncPreview()}><FolderSync size={17}/>{syncText.button}</button><button disabled={active.protocol === 's3'} title={active.protocol === 's3' ? s3Copy[language].readOnly : undefined} onClick={createDirectory}><FolderPlus size={17}/>{t.newFolder}</button><button disabled={active.protocol === 's3'} title={active.protocol === 's3' ? s3Copy[language].readOnly : undefined} onClick={() => void uploadDirectory()}><FolderUp size={16}/>{t.uploadFolder}</button><button className="primary" onClick={() => void uploadFiles()}><Upload size={16}/>{t.upload}</button>
          </div>
          <div className="path-toolbar">
            <button aria-label={accessibilityCopy[language].back} title={accessibilityCopy[language].back} disabled={directoryHistoryIndex <= 0} onClick={() => void navigateDirectoryHistory(-1)}><ChevronLeft size={18}/></button><button aria-label={accessibilityCopy[language].forward} title={accessibilityCopy[language].forward} disabled={directoryHistoryIndex < 0 || directoryHistoryIndex >= directoryHistory.length - 1} onClick={() => void navigateDirectoryHistory(1)}><ChevronRight size={18}/></button>
            <button className="parent-directory-button" aria-label={accessibilityCopy[language].parent} title={accessibilityCopy[language].parent} disabled={path === '/'} onClick={() => void navigateDirectory(active, parentPath(path))}><ArrowUpToLine size={18}/></button>
            <div className="path-field"><span>{t.path}</span><input value={path} onChange={(event) => setPath(event.target.value)} onKeyDown={(event) => event.key === 'Enter' && void navigateDirectory()} /></div>
            <button className="copy-path-button" aria-label={t.copyPath} title={t.copyPath} onClick={() => void copyCurrentPath()}>{pathCopied ? <Check size={17}/> : <Copy size={17}/>}</button>
          </div>
          {error && <div className="error-banner"><strong>{t.error}:</strong> {error}</div>}
          <div className="connection-strip"><span className="online-dot" />{active.name}<span>·</span><span>{active.username}@{active.host}</span><span className="connected">{t.connected}</span></div>
          {viewMode === 'list' && <div className="file-table" role="table" style={{ '--column-name': `${columnWidths.name}px`, '--column-size': `${columnWidths.size}px`, '--column-modified': `${columnWidths.modified}px`, '--column-permissions': `${columnWidths.permissions}px` } as React.CSSProperties}>
            <div className="file-header" role="row">
              <ResizableColumnHeader label={t.name} column="name" width={columnWidths.name} resizeLabel={columnsText.resize} onStart={startColumnResize} onAdjust={adjustColumnWidth}/>
              <ResizableColumnHeader label={t.size} column="size" width={columnWidths.size} resizeLabel={columnsText.resize} onStart={startColumnResize} onAdjust={adjustColumnWidth}/>
              <ResizableColumnHeader label={t.modified} column="modified" width={columnWidths.modified} resizeLabel={columnsText.resize} onStart={startColumnResize} onAdjust={adjustColumnWidth}/>
              <ResizableColumnHeader label={columnsText.permissions} column="permissions" width={columnWidths.permissions} resizeLabel={columnsText.resize} onStart={startColumnResize} onAdjust={adjustColumnWidth}/><span />
            </div>
            {filteredEntries.map((entry) => { const remotePath = joinPath(path, entry.name); const selected = selectedRemoteFile?.connectionId === active.id && selectedRemoteFile.remotePath === remotePath; const ready = dragExport?.connectionId === active.id && dragExport.remotePath === remotePath; const mutationsAvailable = active.protocol !== 's3'; return <div className={`file-row interactive ${selected ? 'selected' : ''} ${ready ? 'drag-ready' : ''}`} key={entry.name} role="row" aria-selected={selected} tabIndex={0} draggable={entry.file_type !== 'Symlink'} onClick={() => scheduleRemoteDragPreparation(entry)} onKeyDown={(event) => { if (event.key === ' ') { event.preventDefault(); scheduleRemoteDragPreparation(entry); } if (event.key === 'Enter') { entry.file_type === 'Directory' ? void navigateDirectory(active, remotePath) : void downloadFile(entry); } }} onDragStart={(event) => startRemoteDrag(event, entry)} onDoubleClick={() => { cancelScheduledDragPreparation(); entry.file_type === 'Directory' ? void navigateDirectory(active, remotePath) : void downloadFile(entry); }}>
              <span className="file-name">{entry.file_type === 'Directory' ? <Folder fill="currentColor" size={18}/> : <Cloud size={18}/>} {entry.name}</span><span>{entry.file_type === 'Directory' ? '—' : formatBytes(entry.size)}</span><span>{entry.modified ?? '—'}</span><span className="permissions-cell">{entry.permissions ?? '—'}</span>{mutationsAvailable ? <button aria-label={accessibilityCopy[language].more} onClick={(event) => { event.stopPropagation(); void manageEntry(entry); }}><MoreHorizontal size={18}/></button> : <span/>}
            </div>; })}
          </div>}
          {viewMode === 'icons' && <div className="icon-grid" role="grid">
            {filteredEntries.length === 0 && <p className="view-empty">{viewsText.empty}</p>}
            {filteredEntries.map((entry) => { const remotePath = joinPath(path, entry.name); const selected = selectedRemoteFile?.connectionId === active.id && selectedRemoteFile.remotePath === remotePath; const ready = dragExport?.connectionId === active.id && dragExport.remotePath === remotePath; const mutationsAvailable = active.protocol !== 's3'; return <div className={`icon-entry ${selected ? 'selected' : ''} ${ready ? 'drag-ready' : ''}`} role="gridcell" key={entry.name} draggable={entry.file_type !== 'Symlink'} onDragStart={(event) => startRemoteDrag(event, entry)}>
              <button className="icon-entry-main" title={entry.name} onClick={() => scheduleRemoteDragPreparation(entry)} onDoubleClick={() => { cancelScheduledDragPreparation(); entry.file_type === 'Directory' ? void navigateDirectory(active, remotePath) : void downloadFile(entry); }} onKeyDown={(event) => { if (event.key !== 'Enter') return; entry.file_type === 'Directory' ? void navigateDirectory(active, remotePath) : void downloadFile(entry); }}>
                {entry.file_type === 'Directory' ? <Folder className="entry-art folder-art" fill="currentColor" size={46}/> : <File className="entry-art" size={44}/>}
                <strong>{entry.name}</strong><small>{entry.file_type === 'Directory' ? entry.permissions ?? '—' : formatBytes(entry.size)}</small>
              </button>
              {mutationsAvailable && <button className="icon-entry-more" aria-label={accessibilityCopy[language].more} onClick={(event) => { event.stopPropagation(); void manageEntry(entry); }}><MoreHorizontal size={16}/></button>}
            </div>; })}
          </div>}
          {viewMode === 'columns' && <div className="column-browser" role="listbox" aria-label={viewsText.columns}>
            {columnLevels.map((level, levelIndex) => {
              const visibleLevelEntries = level.entries.filter((entry) => entry.name.toLowerCase().includes(query.toLowerCase()));
              return <section className="directory-column" key={`${level.path}-${levelIndex}`} aria-label={level.path}>
                <div className="directory-column-title" title={level.path}>{level.path}</div>
                {visibleLevelEntries.length === 0 && <p className="view-empty">{viewsText.empty}</p>}
                {visibleLevelEntries.map((entry) => { const remotePath = joinPath(level.path, entry.name); const selectedForDrag = selectedRemoteFile?.connectionId === active.id && selectedRemoteFile.remotePath === remotePath; const ready = dragExport?.connectionId === active.id && dragExport.remotePath === remotePath; const mutationsAvailable = active.protocol !== 's3'; return <div className={`column-entry ${level.selectedName === entry.name || selectedForDrag ? 'selected' : ''} ${ready ? 'drag-ready' : ''}`} key={entry.name} role="option" aria-selected={level.selectedName === entry.name || selectedForDrag} draggable={entry.file_type !== 'Symlink'} onDragStart={(event) => startRemoteDrag(event, entry, level.path)}>
                  <button className="column-entry-main" title={entry.name} onClick={(event) => { if (event.detail <= 1) { cancelScheduledDragPreparation(); setSelectedRemoteFile({ connectionId: active.id, remotePath }); void openColumnEntry(levelIndex, entry); } }} onDoubleClick={() => { cancelScheduledDragPreparation(); if (entry.file_type !== 'Directory') void downloadFile(entry, level.path); }}>
                    {entry.file_type === 'Directory' ? <Folder fill="currentColor" size={17}/> : <File size={16}/>}<span>{entry.name}</span>{entry.file_type === 'Directory' ? <ChevronRight size={14}/> : <small>{formatBytes(entry.size)}</small>}
                  </button>
                  {mutationsAvailable && <button className="column-entry-more" aria-label={accessibilityCopy[language].more} onClick={(event) => { event.stopPropagation(); void manageEntry(entry, level.path); }}><MoreHorizontal size={15}/></button>}
                </div>; })}
              </section>;
            })}
          </div>}
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
      <div className="transfer-heading"><span>{t.status}</span><small>{transfers.filter((item) => item.status === 'Running').length + remoteEdits.length + (dragPreparingPath ? 1 : 0)} active</small><span className="transfer-heading-spacer"/>{transfers.some((item) => item.status !== 'Running') && <button className="icon-button" aria-label={queueText.clearTransferHistory} title={queueText.clearTransferHistory} onClick={() => void clearTransferHistory()}><Trash2 size={15}/></button>}<button className="icon-button" aria-label={transferPanelCollapsed ? queueText.expand : queueText.collapse} title={transferPanelCollapsed ? queueText.expand : queueText.collapse} aria-expanded={!transferPanelCollapsed} onClick={() => setTransferPanelCollapsed((current) => !current)}>{transferPanelCollapsed ? <ChevronUp size={17}/> : <ChevronDown size={17}/>}</button></div>
      <div className="transfer-list">
        {directoryProgress && directoryProgress.status !== 'completed' && <div className="directory-progress"><span>{directoryProgress.completedFiles} / {directoryProgress.totalFiles || '…'}</span><strong>{directoryProgress.currentPath}</strong><button onClick={() => void controlDirectoryTransfer(directoryPaused ? 'resume' : 'pause')}>{directoryPaused ? t.resume : t.pause}</button><button onClick={() => void controlDirectoryTransfer('cancel')}>{t.cancel}</button></div>}
        {dragPreparingPath && <div className="drag-export-progress"><LoaderCircle className="spinning" size={15}/><strong>{dragPreparingPath.split('/').pop()}</strong><span>{dragText.preparing}</span></div>}
        {remoteEdits.map((edit) => <div className={`remote-edit-row ${edit.status}`} key={edit.editId}><Pencil size={15}/><strong>{edit.name}</strong><span>{edit.status === 'waiting' ? editText.waiting : edit.status === 'failed' ? edit.detail : editText.watching}</span><small title={edit.remotePath}>{edit.remotePath}</small><button className="icon-button" aria-label={editText.stop} title={editText.stop} onClick={() => void closeRemoteEdit(edit)}><Trash2 size={14}/></button></div>)}
        {transfers.length === 0 && remoteEdits.length === 0 && !dragPreparingPath ? <p className="muted">Transfers will appear here.</p> : transfers.slice(0, 8).map((transfer) => <div className="transfer-row" key={transfer.id}>
          <span className={`transfer-status ${transfer.status.toLowerCase()}`}/><strong>{transfer.name}</strong>
          <span>{transfer.totalBytes ? `${Math.round(((transfer.transferredBytes ?? 0) / transfer.totalBytes) * 100)}%` : transfer.direction}</span>
          <span>{transfer.speed ? `${formatBytes(transfer.speed)}/s · ${p2.eta} ${formatDuration(transfer.etaSeconds)}` : transfer.detail}</span>
          <span className="transfer-actions">
            {transfer.status === 'Running' && <><button onClick={() => void controlTransfer(transfer, pausedTransfers.has(transfer.id) ? 'resume' : 'pause')}>{pausedTransfers.has(transfer.id) ? t.resume : t.pause}</button><button onClick={() => void controlTransfer(transfer, 'cancel')}>{t.cancel}</button></>}
            {transfer.status === 'Failed' && transfer.localPath ? <button onClick={() => void retryTransfer(transfer)}>{t.retry}</button> : transfer.status !== 'Running' ? transfer.status : null}
          </span>
        </div>)}
      </div>
    </section>
    {showConnect && <ConnectSheet mode={connectSheetMode} bookmark={connectingBookmark} initialKeyPath={selectedKeyPath} defaultProtocol={preferences.defaultProtocol} t={t} phaseCopy={p1} localCopy={bookmarkLocalText} s3Text={s3Copy[language]} onClose={() => setShowConnect(false)} onSaved={(connection) => {
      setConnections((current) => [connection, ...current.filter((item) => item.id !== connection.id)]);
      setActive((current) => {
        if (current?.id !== connection.id) return current;
        const targetChanged = connectionTargetChanged(current, connection);
        return targetChanged ? null : { ...current, name: connection.name, initialPath: connection.initialPath, keyPath: connection.keyPath, localDirectory: connection.localDirectory, tags: connection.tags };
      });
      setNotice(t.bookmarkSaved);
      setShowConnect(false);
    }} onConnected={(connection) => { setConnections((current) => [connection, ...current.filter((item) => item.id !== connection.id)]); void Promise.all([invoke('bookmark_save', { bookmark: connection }), invoke('connection_history_record', { bookmark: connection })]).then(() => invoke<ConnectionHistory[]>('connection_history_list')).then(setHistory).catch((reason) => setError(String(reason))); setActive(connection); setShowConnect(false); void navigateDirectory(connection, connection.initialPath, true); }} />}
    {showPreferences && <PreferencesSheet value={preferences} language={language} t={t} onClose={() => setShowPreferences(false)} onSave={(next) => { setPreferences(next); setShowPreferences(false); }} />}
    {syncLocalDirectory && <SyncPreviewSheet preview={syncPreview} localDirectory={syncLocalDirectory} remoteDirectory={path} direction={syncDirection} busy={syncPreviewBusy} executionBusy={syncExecutionBusy} error={syncPreviewError} exclusions={syncExclusions} conflictChoices={syncConflictChoices} progress={syncExecutionProgress} result={syncExecutionResult} history={syncHistory} t={t} text={syncText} onClose={() => { if (syncExecutionBusy) return; setSyncLocalDirectory(''); setSyncPreview(null); }} onDirection={(direction) => { setSyncDirection(direction); setSyncExecutionResult(null); void calculateSyncPreview(syncLocalDirectory, direction); }} onExclusions={(value) => { setSyncExclusions(value); setSyncPreview(null); setSyncExecutionResult(null); }} onConflict={(itemPath, choice) => setSyncConflictChoices((current) => ({ ...current, [itemPath]: choice }))} onRefresh={() => void calculateSyncPreview()} onExecute={() => void executeSync()} onCancelExecution={() => void cancelSyncExecution()} onClearHistory={() => void clearSyncHistory()} />}
  </main>;
}

function SyncPreviewSheet({ preview, localDirectory, remoteDirectory, direction, busy, executionBusy, error, exclusions, conflictChoices, progress, result, history, t, text, onClose, onDirection, onExclusions, onConflict, onRefresh, onExecute, onCancelExecution, onClearHistory }: { preview: SyncPreview | null; localDirectory: string; remoteDirectory: string; direction: SyncDirection; busy: boolean; executionBusy: boolean; error: string; exclusions: string; conflictChoices: Record<string, SyncConflictChoice>; progress: SyncExecutionProgress | null; result: SyncExecutionResult | null; history: SyncHistory[]; t: typeof copy[keyof typeof copy]; text: typeof syncCopy[keyof typeof syncCopy]; onClose: () => void; onDirection: (direction: SyncDirection) => void; onExclusions: (value: string) => void; onConflict: (path: string, choice: SyncConflictChoice) => void; onRefresh: () => void; onExecute: () => void; onCancelExecution: () => void; onClearHistory: () => void }) {
  const actionLabel = (action: SyncAction) => ({ upload: text.upload, download: text.download, createRemoteDirectory: text.createRemoteDirectory, createLocalDirectory: text.createLocalDirectory, conflict: text.conflict, destinationOnly: text.destinationOnlyAction })[action];
  const executableCount = preview?.items.filter((item) => item.action !== 'destinationOnly' && (item.action !== 'conflict' || (!item.isDirectory && conflictChoices[item.path] === 'source'))).length ?? 0;
  const statusLabel = (status: string) => status === 'Completed' ? text.completed : status === 'Cancelled' ? text.cancelled : text.failed;
  return <div className="modal-backdrop" role="presentation"><section className="connect-sheet sync-sheet">
    <div className="sheet-title"><div><h2>{text.title}</h2><p>{text.detail}</p></div><button type="button" disabled={executionBusy} onClick={onClose}>×</button></div>
    <div className="sync-paths"><span><strong>Local</strong>{localDirectory}</span><span><strong>Remote</strong>{remoteDirectory}</span></div>
    <div className="sync-controls"><label>{text.direction}<select value={direction} disabled={executionBusy} onChange={(event) => onDirection(event.target.value as SyncDirection)}><option value="localToRemote">{text.localToRemote}</option><option value="remoteToLocal">{text.remoteToLocal}</option></select></label><button onClick={onRefresh} disabled={busy || executionBusy}>{busy && <LoaderCircle className="spinning" size={15}/>} {text.refresh}</button></div>
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

function ConnectSheet({ mode, bookmark, initialKeyPath, defaultProtocol, t, phaseCopy, localCopy, s3Text, onClose, onSaved, onConnected }: { mode: 'connect' | 'edit'; bookmark: Connection | null; initialKeyPath: string; defaultProtocol: Protocol; t: typeof copy[keyof typeof copy]; phaseCopy: typeof phaseOneCopy[keyof typeof phaseOneCopy]; localCopy: typeof bookmarkLocalCopy[keyof typeof bookmarkLocalCopy]; s3Text: typeof s3Copy[keyof typeof s3Copy]; onClose: () => void; onSaved: (connection: Connection) => void; onConnected: (connection: Connection) => void }) {
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
  const [keyPath, setKeyPath] = useState(initialKeyPath || bookmark?.keyPath || '');
  const [tags, setTags] = useState(bookmark?.tags ?? '');
  const [localDirectory, setLocalDirectory] = useState(bookmark?.localDirectory ?? '');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  function updateProtocol(value: Protocol) { setProtocol(value); setPort(defaultPort(value)); }
  async function selectLocalDirectory() {
    const selected = await open({ multiple: false, directory: true });
    if (selected && !Array.isArray(selected)) setLocalDirectory(selected);
  }
  useEffect(() => { if (bookmark) void invoke<string | null>('credential_load', { bookmarkId: bookmark.id }).then((saved) => { if (!saved) return; if (bookmark.protocol === 's3') { try { const value = JSON.parse(saved) as { accessKeyId?: string; secretAccessKey?: string; sessionToken?: string }; setUsername(value.accessKeyId ?? ''); setPassword(value.secretAccessKey ?? ''); setS3SessionToken(value.sessionToken ?? ''); } catch { setUsername(''); setPassword(''); } } else setPassword(saved); }).catch(() => undefined); }, [bookmark]);
  async function submit(event: React.FormEvent) {
    event.preventDefault(); setBusy(true); setError('');
    try {
      const endpointUnchanged = bookmark?.protocol === protocol && bookmark.host === host && bookmark.port === port;
      let hostKey = endpointUnchanged ? bookmark?.hostKey : undefined;
      if (mode === 'connect') {
        hostKey = protocol === 'sftp' ? await invoke<string>('sftp_probe_host_key', { host, port }) : undefined;
        if (bookmark?.hostKey && endpointUnchanged && bookmark.hostKey !== hostKey) { setError(t.hostKeyChanged); return; }
        if (hostKey && (!bookmark?.hostKey || !endpointUnchanged) && !window.confirm(t.trustHostKey.replace('{{fingerprint}}', hostKey))) return;
      }
      const connection: Connection = { id: bookmark?.id ?? crypto.randomUUID(), name: bookmarkName.trim() || host, protocol, host, port, username: protocol === 's3' ? '' : username, initialPath: initialDirectory.trim() || '/', keyPath: keyPath || undefined, hostKey, localDirectory: localDirectory || undefined, tags, s3Region: protocol === 's3' ? s3Region.trim() : undefined, s3Endpoint: protocol === 's3' && s3Endpoint.trim() ? s3Endpoint.trim() : undefined, s3ForcePathStyle: protocol === 's3' && s3ForcePathStyle };
      const storedCredential = protocol === 's3' ? JSON.stringify({ accessKeyId: username, secretAccessKey: password, sessionToken: s3SessionToken || undefined }) : password;
      if (mode === 'edit') {
        if (password) await invoke('credential_save', { bookmarkId: connection.id, password: storedCredential });
        await invoke('bookmark_save', { bookmark: connection });
        onSaved(connection);
        return;
      }
      await invoke('connection_connect', { request: { connectionId: connection.id, protocol, host, port, username, password: password || null, keyPath: keyPath || null, passphrase: null, expectedHostKey: hostKey ?? null, initialPath: connection.initialPath, s3Region: connection.s3Region ?? null, s3Endpoint: connection.s3Endpoint ?? null, s3SessionToken: s3SessionToken || null, s3ForcePathStyle: connection.s3ForcePathStyle ?? false } });
      if (password) await invoke('credential_save', { bookmarkId: connection.id, password: storedCredential });
      onConnected(connection);
    } catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  }
  return <div className="modal-backdrop" role="presentation"><form className="connect-sheet bookmark-sheet" onSubmit={submit}>
    <div className="sheet-title"><div><h2>{mode === 'edit' ? t.editBookmarkTitle : t.connectTitle}</h2><p>{mode === 'edit' ? t.editBookmark : t.connect}</p></div><button type="button" onClick={onClose}>×</button></div>
    <div className="bookmark-sheet-scroll">
      <label>{t.bookmarkName}<input required value={bookmarkName} onChange={(event) => setBookmarkName(event.target.value)} placeholder={t.bookmarkNameHint}/></label>
      <label>{t.protocol}<select value={protocol} onChange={(event) => updateProtocol(event.target.value as Protocol)}><option value="sftp">SFTP</option><option value="ftp">FTP</option><option value="ftps">Explicit FTPS</option><option value="webdav">WebDAV (HTTPS)</option><option value="s3">Amazon S3 / S3-compatible (read-only)</option></select></label>
      <div className="form-grid"><label>{protocol === 's3' ? s3Text.bucket : t.host}<input required value={host} onChange={(event) => setHost(event.target.value)} placeholder={protocol === 's3' ? 'example-bucket' : 'example.com'}/></label>{protocol !== 's3' && <label>{t.port}<input required type="number" value={port} onChange={(event) => setPort(Number(event.target.value))}/></label>}</div>
      {protocol === 'webdav' && <p className="protocol-security-hint">{phaseCopy.webdavHint}</p>}
      <label>{t.initialDirectory}<input required value={initialDirectory} onChange={(event) => setInitialDirectory(event.target.value)} placeholder={t.initialDirectoryHint}/></label>
      {protocol === 's3' && <><p className="protocol-security-hint">{s3Text.readOnly}</p><div className="form-grid"><label>{s3Text.region}<input required value={s3Region} onChange={(event) => setS3Region(event.target.value)} placeholder="ap-northeast-1"/></label><label>{s3Text.endpoint}<input type="url" value={s3Endpoint} onChange={(event) => setS3Endpoint(event.target.value)} placeholder="https://s3.example.com"/></label></div></>}
      <label>{protocol === 's3' ? s3Text.accessKey : t.user}<input required value={username} onChange={(event) => setUsername(event.target.value)}/></label><label>{protocol === 's3' ? s3Text.secretKey : t.password}<input required={protocol === 'webdav' || protocol === 's3'} type="password" value={password} onChange={(event) => setPassword(event.target.value)}/></label>
      {protocol === 's3' && <><label>{s3Text.sessionToken}<input type="password" value={s3SessionToken} onChange={(event) => setS3SessionToken(event.target.value)}/></label><label className="check-row"><input type="checkbox" checked={s3ForcePathStyle} onChange={(event) => setS3ForcePathStyle(event.target.checked)}/><span>{s3Text.pathStyle}</span></label></>}
      {protocol === 'sftp' && <label>{t.key}<input value={keyPath} onChange={(event) => setKeyPath(event.target.value)} placeholder="~/.ssh/id_ed25519"/></label>}
      <label>{phaseCopy.tags}<input value={tags} onChange={(event) => setTags(event.target.value)} placeholder={phaseCopy.tagHint}/></label>
      <section className="bookmark-local-directory-section">
        <div className="bookmark-local-directory-copy"><strong>{localCopy.title}</strong><p>{localCopy.detail}</p></div>
        <div className="bookmark-local-directory-picker"><div className={`local-directory-path ${localDirectory ? '' : 'empty'}`} title={localDirectory || localCopy.none}><Folder size={16}/><span>{localDirectory || localCopy.none}</span></div><button type="button" onClick={() => void selectLocalDirectory()}>{localCopy.select}</button>{localDirectory && <button type="button" onClick={() => setLocalDirectory('')}>{localCopy.clear}</button>}</div>
      </section>
      {error && <p className="form-error">{error}</p>}
    </div>
    <div className="form-actions"><button type="button" onClick={onClose}>{t.cancel}</button><button className="primary" disabled={busy}>{busy && <LoaderCircle className="spinning" size={16}/>} {mode === 'edit' ? t.saveBookmark : t.start}</button></div>
  </form></div>;
}

function PreferencesSheet({ value, language, t, onClose, onSave }: { value: Preferences; language: Language; t: typeof copy[keyof typeof copy]; onClose: () => void; onSave: (preferences: Preferences) => void }) {
  const [draft, setDraft] = useState(value);
  const text = preferencesCopy[language];
  async function selectEditor() {
    const selected = await open({ multiple: false, directory: false, filters: [{ name: 'macOS Applications', extensions: ['app'] }] });
    if (selected && !Array.isArray(selected)) setDraft((current) => ({ ...current, editorPath: selected }));
  }
  return <div className="modal-backdrop" role="presentation"><form className="connect-sheet preferences-sheet" onSubmit={(event) => { event.preventDefault(); onSave(draft); }}>
    <div className="sheet-title"><div><h2>{text.title}</h2><p>{text.detail}</p></div><button type="button" onClick={onClose}>×</button></div>
    <div className="preferences-sheet-scroll">
      <fieldset><legend>{text.general}</legend>
        <label>{text.language}<select value={draft.language} onChange={(event) => setDraft((current) => ({ ...current, language: event.target.value as Language }))}><option value="ja">日本語</option><option value="en">English</option><option value="zh-CN">简体中文</option></select></label>
        <label>{text.defaultProtocol}<select value={draft.defaultProtocol} onChange={(event) => setDraft((current) => ({ ...current, defaultProtocol: event.target.value as Protocol }))}><option value="sftp">SFTP</option><option value="ftp">FTP</option><option value="ftps">Explicit FTPS</option><option value="webdav">WebDAV (HTTPS)</option><option value="s3">Amazon S3 / S3-compatible</option></select></label>
      </fieldset>
      <fieldset><legend>{text.appearance}</legend><label>{text.theme}<select value={draft.theme} onChange={(event) => setDraft((current) => ({ ...current, theme: event.target.value as Preferences['theme'] }))}><option value="system">{text.system}</option><option value="light">{text.light}</option><option value="dark">{text.dark}</option></select></label></fieldset>
      <fieldset><legend>{text.editor}</legend><p className="preferences-field-detail">{text.editorDetail}</p><div className="editor-picker"><input readOnly value={draft.editorPath} placeholder={text.noEditor}/><button type="button" onClick={() => void selectEditor()}>{text.chooseEditor}</button>{draft.editorPath && <button type="button" onClick={() => setDraft((current) => ({ ...current, editorPath: '' }))}>{text.clearEditor}</button>}</div></fieldset>
      <fieldset><legend>{text.transfers}</legend>
        <label>{text.conflictPolicy}<select value={draft.conflictPolicy} onChange={(event) => setDraft((current) => ({ ...current, conflictPolicy: event.target.value as Preferences['conflictPolicy'] }))}><option value="ask">{text.ask}</option><option value="overwrite">{text.overwrite}</option><option value="skip">{text.skip}</option></select></label>
        <label className="check-row"><input type="checkbox" checked={draft.transferNotifications} onChange={(event) => setDraft((current) => ({ ...current, transferNotifications: event.target.checked }))}/><span>{text.notifications}</span></label>
      </fieldset>
      <fieldset><legend>{text.security}</legend><label className="check-row"><input type="checkbox" checked={draft.confirmDelete} onChange={(event) => setDraft((current) => ({ ...current, confirmDelete: event.target.checked }))}/><span>{text.confirmDelete}</span></label></fieldset>
    </div>
    <div className="form-actions"><button type="button" onClick={onClose}>{t.cancel}</button><button className="primary">{text.save}</button></div>
  </form></div>;
}

function KeyManager({ t, onClose, onUse }: { t: typeof copy[keyof typeof copy]; onClose: () => void; onUse: (keyPath: string) => void }) {
  const [keys, setKeys] = useState<SshKey[]>([]);
  const [error, setError] = useState('');
  const [loaded, setLoaded] = useState(false);
  async function load() { setError(''); try { setKeys(await invoke<SshKey[]>('ssh_keys_list')); } catch (reason) { setError(String(reason)); } finally { setLoaded(true); } }
  useEffect(() => { void load(); }, []);
  return <section className="connect-sheet key-sheet standalone-key-sheet">
    <div className="sheet-title"><div><h2>{t.keyManager}</h2><p>{t.keyHint}</p></div><button type="button" onClick={onClose}>×</button></div>
    {error && <p className="form-error">{error}</p>}
    <div className="key-list">
      {loaded && keys.length === 0 && <p className="muted key-empty">{t.noKeys}</p>}
      {keys.map((key) => <div className="key-row" key={key.path}><KeyRound size={19}/><div><strong>{key.name}</strong><small>{key.kind} · {key.path}</small>{key.publicKeyPath && <em>{t.pairedKey}</em>}</div><button className="primary" onClick={() => onUse(key.path)}>{t.useKey}</button></div>)}
    </div>
    <div className="form-actions"><button type="button" onClick={() => void load()}>{t.refresh}</button><button type="button" onClick={onClose}>{t.cancel}</button></div>
  </section>;
}

export function SshKeyManagerWindow() {
  const preferences = loadPreferences();
  const t = copy[preferences.language];
  async function closeWindow() { await getCurrentWebviewWindow().close(); }
  async function selectKey(keyPath: string) {
    await emitTo('main', 'ssh-key://selected', keyPath);
    await closeWindow();
  }
  return <main className="key-manager-window"><KeyManager t={t} onClose={() => void closeWindow()} onUse={(keyPath) => void selectKey(keyPath)}/></main>;
}
