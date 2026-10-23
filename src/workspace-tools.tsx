import { createContext, useCallback, useContext, useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Check, Folder, LoaderCircle, Star, Trash2, X } from 'lucide-react';

type Language = 'ja' | 'en' | 'zh-CN';
export type WorkspaceLocation = { kind: 'local'; path: string } | { kind: 'remote'; connectionId: string; path: string };
export type NavigationRequest = { id: number; path: string; connectionId?: string };
type SavedPlace = { id: string; label: string; kind: 'local' | 'remote'; bookmarkId: string; path: string };

export const workspaceToolsCopy = {
  ja: { places: 'よく使う場所', add: '現在のフォルダを登録', edit: '場所を編集', name: '名称', save: '保存', remove: '登録を解除', close: '閉じる', link: '連動ナビゲーション：現在の左右のフォルダを基準にする', unlink: '連動ナビゲーションを解除', resume: '連動ナビゲーションを再開（現在のフォルダを基準にする）', paused: '連動ナビゲーションを一時停止', outsideRoot: '基準フォルダの外へ移動したか、接続先が変更されました。', missing: '対応するフォルダがありません。', ambiguous: '対応先に同名の項目が複数あります。', notDirectory: '対応先はフォルダではないか、シンボリックリンクです。', invalidPath: '対応するパスを安全に表現できません。', failed: '対応先を開けませんでした。', context: '右クリックで編集' },
  en: { places: 'Places', add: 'Save current folder', edit: 'Edit place', name: 'Name', save: 'Save', remove: 'Remove place', close: 'Close', link: 'Link navigation using the current folders as roots', unlink: 'Unlink navigation', resume: 'Resume linked navigation using the current folders as roots', paused: 'Linked navigation paused', outsideRoot: 'You left a root folder or changed the connection.', missing: 'The matching folder does not exist.', ambiguous: 'The destination contains duplicate names.', notDirectory: 'The destination is not a folder or is a symbolic link.', invalidPath: 'The matching path cannot be represented safely.', failed: 'Could not open the matching folder.', context: 'Right-click to edit' },
  'zh-CN': { places: '常用位置', add: '收藏当前文件夹', edit: '编辑位置', name: '名称', save: '保存', remove: '移除收藏', close: '关闭', link: '联动导航：以当前左右文件夹为起点', unlink: '取消联动导航', resume: '恢复联动导航（以当前文件夹为起点）', paused: '联动导航已暂停', outsideRoot: '已离开起始文件夹或切换连接。', missing: '对应的文件夹不存在。', ambiguous: '目标中存在多个同名项目。', notDirectory: '目标不是文件夹或是符号链接。', invalidPath: '无法安全表示对应路径。', failed: '无法打开对应文件夹。', context: '右键编辑' },
} as const;

type PlacesContextValue = {
  places: SavedPlace[];
  language: Language;
  formatPath: (path: string) => string;
  save: (place: Omit<SavedPlace, 'id'> & { id?: string }) => Promise<void>;
  remove: (id: string) => Promise<void>;
};
const PlacesContext = createContext<PlacesContextValue | null>(null);

export function WorkspacePlacesProvider({ language, formatPath, onError, children }: {
  language: Language; formatPath: (path: string) => string; onError: (error: string) => void; children: React.ReactNode;
}) {
  const [places, setPlaces] = useState<SavedPlace[]>([]);
  const request = useRef(0);
  const mounted = useRef(false);
  const refresh = useCallback(async () => {
    const sequence = ++request.current;
    const next = await invoke<SavedPlace[]>('workspace_places_list');
    if (mounted.current && sequence === request.current) setPlaces(Array.isArray(next) ? next : []);
  }, []);
  useEffect(() => {
    mounted.current = true;
    let cancelled = false;
    let dispose: (() => void) | undefined;
    void refresh().catch((error) => { if (mounted.current) onError(String(error)); });
    void listen('workspace://places-changed', () => { void refresh().catch((error) => { if (!cancelled) onError(String(error)); }); })
      .then((unlisten) => { if (!cancelled) dispose = unlisten; else unlisten(); })
      .catch((error) => { if (!cancelled) onError(String(error)); });
    return () => { cancelled = true; mounted.current = false; dispose?.(); };
  }, [onError, refresh]);
  const save = async (place: Omit<SavedPlace, 'id'> & { id?: string }) => {
    await invoke('workspace_place_save', { request: { ...place, id: place.id ?? null } });
    await refresh();
  };
  const remove = async (id: string) => {
    await invoke('workspace_place_delete', { id });
    await refresh();
  };
  return <PlacesContext.Provider value={{ places, language, formatPath, save, remove }}>{children}</PlacesContext.Provider>;
}

export function PlacesBar({ location, onNavigate }: { location: WorkspaceLocation | null; onNavigate: (path: string) => void }) {
  const context = useContext(PlacesContext);
  const [editor, setEditor] = useState<{ place?: SavedPlace; label: string; location: WorkspaceLocation; x: number; y: number } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const popup = useRef<HTMLDivElement>(null);
  const anchor = useRef<HTMLElement | null>(null);
  const editorOpen = editor !== null;
  useEffect(() => {
    if (!editorOpen) return;
    const close = (event: PointerEvent) => { if (!popup.current?.contains(event.target as Node) && !busy) setEditor(null); };
    const escape = (event: KeyboardEvent) => { if (event.key === 'Escape' && !busy) { setEditor(null); anchor.current?.focus(); } };
    document.addEventListener('pointerdown', close);
    document.addEventListener('keydown', escape);
    return () => { document.removeEventListener('pointerdown', close); document.removeEventListener('keydown', escape); };
  }, [editorOpen, busy]);
  if (!context) return null;
  const text = workspaceToolsCopy[context.language];
  const bookmarkId = location?.kind === 'remote' ? location.connectionId : '';
  const scoped = context.places.filter((place) => place.kind === location?.kind && place.bookmarkId === bookmarkId);
  const current = scoped.find((place) => place.path === location?.path);
  function openEditor(element: HTMLElement, place?: SavedPlace) {
    if (!location || !context) return;
    anchor.current = element;
    const rect = element.getBoundingClientRect();
    const label = place?.label ?? (context.formatPath(location.path).split('/').filter(Boolean).pop() || '/');
    setError('');
    setEditor({ place, label, location: place ? { ...location, path: place.path } : location, x: Math.max(8, Math.min(window.innerWidth - 280, rect.left)), y: Math.max(8, Math.min(window.innerHeight - 190, rect.bottom + 4)) });
  }
  async function submit(remove: boolean) {
    if (!editor || !context || busy) return;
    setBusy(true); setError('');
    try {
      if (remove && editor.place) await context.remove(editor.place.id);
      else await context.save({ id: editor.place?.id, label: editor.label.trim(), kind: editor.location.kind, bookmarkId: editor.location.kind === 'remote' ? editor.location.connectionId : '', path: editor.location.path });
      setEditor(null); anchor.current?.focus();
    } catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  }
  return <>
    <nav className="places-bar" aria-label={text.places}>
      <button className="place-add" title={current ? text.edit : text.add} aria-label={current ? text.edit : text.add} disabled={!location} onClick={(event) => openEditor(event.currentTarget, current)}><Star size={13} fill={current ? 'currentColor' : 'none'}/></button>
      <div className="places-items">{scoped.map((place) => <button className="place-chip" key={place.id} aria-current={place.path === location?.path ? 'location' : undefined} title={`${place.label}\n${place.kind === 'remote' ? context.formatPath(place.path) : place.path}\n${text.context}`} onClick={() => onNavigate(place.path)} onContextMenu={(event) => { event.preventDefault(); openEditor(event.currentTarget, place); }}><Folder size={12}/><span>{place.label}</span></button>)}</div>
    </nav>
    {editor && createPortal(<div ref={popup} className="place-editor" role="dialog" aria-label={editor.place ? text.edit : text.add} style={{ left: editor.x, top: editor.y }}>
      <form onSubmit={(event) => { event.preventDefault(); void submit(false); }}>
        <label>{text.name}<input autoFocus maxLength={128} value={editor.label} onChange={(event) => setEditor({ ...editor, label: event.target.value })} onFocus={(event) => event.target.select()} disabled={busy}/></label>
        <small title={editor.location.path}>{editor.location.kind === 'remote' ? context.formatPath(editor.location.path) : editor.location.path}</small>
        {error && <p role="alert">{error}</p>}
        <div className="place-editor-actions">{editor.place && <button type="button" aria-label={text.remove} title={text.remove} disabled={busy} onClick={() => void submit(true)}><Trash2 size={15}/></button>}<span/><button type="button" aria-label={text.close} title={text.close} disabled={busy} onClick={() => { setEditor(null); anchor.current?.focus(); }}><X size={15}/></button><button className="primary" aria-label={text.save} title={text.save} disabled={busy || !editor.label.trim()}>{busy ? <LoaderCircle size={15} className="spinning"/> : <Check size={15}/>}</button></div>
      </form>
    </div>, document.body)}
  </>;
}

export function locationKey(location: WorkspaceLocation | null) { return location ? JSON.stringify([location.kind, location.kind === 'remote' ? location.connectionId : '', location.path]) : ''; }
export function sameLocationSource(left: WorkspaceLocation | null, right: WorkspaceLocation | null) {
  return left?.kind === right?.kind && (left?.kind !== 'remote' || right?.kind !== 'remote' || left.connectionId === right.connectionId);
}
