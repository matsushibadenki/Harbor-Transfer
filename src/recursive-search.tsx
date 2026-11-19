import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { File, Folder, Info, LoaderCircle, Search, Square, X } from 'lucide-react';

type Language = 'ja' | 'en' | 'zh-CN';
type Location = { kind: 'local' | 'remote'; path: string; connectionId?: string };
type Match = { name: string; path: string; isDirectory: boolean };
type Result = { matches: Match[]; inspected: number; directories: number; truncated: boolean; cancelled: boolean };
type Progress = { searchId: string; path: string; inspected: number; directories: number; matches: number };
const copy = {
  ja: { title: 'サブフォルダも検索', query: 'ファイル名', start: '検索', stop: '検索を中止', stopping: '停止待ち…', running: '検索中…', done: '検索完了', cancelled: '中止しました', empty: '該当する項目はありません', idle: 'ファイル名を入力して検索', close: '閉じる', reveal: 'フォルダを表示', limits: '名前の部分一致で検索します。最大500結果・10,000項目・256フォルダ・深さ32。30秒を目安に次の走査を停止します。リンクは追跡しません。停止時は現在の一覧取得が完了するまで待ちます。', partial: '上限に達したため、結果の一部を表示しています', count: '確認済み {{items}}項目 · {{dirs}}フォルダ · {{matches}}件一致', failure: '検索できませんでした' },
  en: { title: 'Search subfolders', query: 'File name', start: 'Search', stop: 'Stop search', stopping: 'Stopping…', running: 'Searching…', done: 'Search complete', cancelled: 'Search stopped', empty: 'No matching items', idle: 'Enter a file name to search', close: 'Close', reveal: 'Show folder', limits: 'Matches part of a name. Limits: 500 results, 10,000 items, 256 folders, depth 32. Stops starting new work after about 30 seconds. Links are not followed. Stopping waits for the current listing to finish.', partial: 'Search limit reached. Showing partial results', count: '{{items}} items inspected · {{dirs}} folders · {{matches}} matches', failure: 'Search failed' },
  'zh-CN': { title: '搜索子文件夹', query: '文件名', start: '搜索', stop: '停止搜索', stopping: '正在停止…', running: '正在搜索…', done: '搜索完成', cancelled: '搜索已停止', empty: '没有匹配的项目', idle: '输入文件名开始搜索', close: '关闭', reveal: '显示文件夹', limits: '按名称部分匹配。最多500条结果、10,000个项目、256个文件夹、深度32层。约30秒后停止新的遍历。不跟踪链接。停止时会等待当前目录读取完成。', partial: '已达到搜索上限，显示部分结果', count: '已检查{{items}}个项目 · {{dirs}}个文件夹 · {{matches}}条匹配', failure: '搜索失败' },
};
type Operation = { id: string; cancelled: boolean; started: boolean; close: boolean; unlisten?: () => void };

export function RecursiveSearchButton({ language, location: currentLocation, disabled = false, showHiddenFiles = true, initialQuery = '', formatPath = value => value, onNavigate }: {
  language: Language; location: Location; disabled?: boolean; showHiddenFiles?: boolean; initialQuery?: string; formatPath?: (path: string) => string; onNavigate: (path: string) => void;
}) {
  const text = copy[language];
  const [open, setOpen] = useState(false);
  const [location, setLocation] = useState(currentLocation);
  const [query, setQuery] = useState('');
  const [phase, setPhase] = useState<'idle' | 'running' | 'stopping' | 'done' | 'error'>('idle');
  const [result, setResult] = useState<Result | null>(null);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [error, setError] = useState('');
  const operation = useRef<Operation | null>(null);
  const mounted = useRef(false);
  const trigger = useRef<HTMLButtonElement>(null);
  const dialog = useRef<HTMLElement>(null);
  const active = phase === 'running' || phase === 'stopping';
  useEffect(() => {
    if (!open) return;
    const input = dialog.current?.querySelector<HTMLInputElement>('input:not(:disabled)');
    (input ?? dialog.current)?.focus();
  }, [open, active]);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      const current = operation.current;
      if (current) { current.cancelled = true; current.unlisten?.(); if (current.started) void invoke('search_cancel', { searchId: current.id }).catch(() => {}); }
    };
  }, []);
  function close() {
    if (operation.current) { operation.current.close = true; stop(); }
    else { setOpen(false); trigger.current?.focus(); }
  }
  function stop() {
    const current = operation.current;
    if (!current || current.cancelled) return;
    current.cancelled = true; setPhase('stopping');
    if (current.started) void invoke('search_cancel', { searchId: current.id }).catch(reason => { if (mounted.current) setError(String(reason)); });
  }
  async function start() {
    if (operation.current || !query.trim()) return;
    const current: Operation = { id: crypto.randomUUID(), cancelled: false, started: false, close: false };
    operation.current = current;
    setResult(null); setProgress(null); setError(''); setPhase('running');
    try {
      current.unlisten = await getCurrentWebviewWindow().listen<Progress>('search://progress', event => {
        if (mounted.current && operation.current === current && event.payload.searchId === current.id) setProgress(event.payload);
      });
      if (current.cancelled || !mounted.current) { if (mounted.current) { setResult({ matches: [], inspected: 0, directories: 0, cancelled: true, truncated: false }); setPhase('done'); } return; }
      current.started = true;
      const found = await invoke<Result>(location.kind === 'local' ? 'local_search' : 'remote_search', location.kind === 'local'
        ? { path: location.path, query: query.trim(), searchId: current.id, showHidden: showHiddenFiles }
        : { request: { connectionId: location.connectionId, path: location.path }, query: query.trim(), searchId: current.id, showHidden: showHiddenFiles });
      if (mounted.current && operation.current === current) { setResult(found); setPhase('done'); }
    } catch (reason) {
      if (mounted.current && operation.current === current) { setError(String(reason)); setPhase('error'); }
    } finally {
      current.unlisten?.();
      if (operation.current === current) operation.current = null;
      if (mounted.current && current.close) { setOpen(false); trigger.current?.focus(); }
    }
  }
  const counts = result ? { ...result, matches: result.matches.length } : progress;
  const countLabel = counts ? text.count.replace('{{items}}', String(counts.inspected)).replace('{{dirs}}', String(counts.directories)).replace('{{matches}}', String(counts.matches)) : '';
  return <><button ref={trigger} disabled={disabled} type="button" className="icon-button recursive-search-trigger" title={text.title} aria-label={text.title} onClick={() => { setLocation(currentLocation); setResult(null); setProgress(null); setPhase('idle'); setError(''); setQuery(initialQuery); setOpen(true); }}><Search size={16}/><span aria-hidden="true">+</span></button>
    {open && createPortal(<div className="modal-backdrop recursive-search-backdrop" onMouseDown={event => { if (event.target === event.currentTarget) close(); }}><section ref={dialog} tabIndex={-1} className="connect-sheet recursive-search-sheet" role="dialog" aria-modal="true" aria-label={text.title} onKeyDown={event => {
      if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); close(); }
      if (event.key === 'Tab') {
        const controls = Array.from(dialog.current?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), [tabindex="0"]') ?? []);
        const first = controls[0], last = controls.at(-1);
        if (!controls.includes(document.activeElement as HTMLElement)) { event.preventDefault(); first?.focus(); }
        else if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
        else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
      }
    }}>
      <header className="sheet-title"><h2>{text.title}</h2><span title={text.limits} tabIndex={0} aria-label={text.limits}><Info size={16}/></span><button type="button" className="icon-button" title={text.close} aria-label={text.close} onClick={close}><X size={18}/></button></header>
      <code className="recursive-search-root" title={formatPath(location.path)}>{formatPath(location.path)}</code>
      <form className="recursive-search-form" onSubmit={event => { event.preventDefault(); void start(); }}><input autoFocus aria-label={text.query} placeholder={text.query} value={query} disabled={active} maxLength={128} autoCorrect="off" autoCapitalize="none" spellCheck={false} onChange={event => setQuery(event.target.value)}/><button type="submit" className="primary" disabled={active || !query.trim()}><Search size={16}/>{text.start}</button></form>
      <div className="recursive-search-status" role="status" aria-live="polite">{active && <LoaderCircle size={15} className="spinning"/>}<span>{phase === 'stopping' ? text.stopping : phase === 'running' ? text.running : phase === 'error' ? text.failure : result ? result.cancelled ? text.cancelled : text.done : text.idle}</span>{active && <button type="button" className="icon-button" disabled={phase === 'stopping'} title={text.stop} aria-label={text.stop} onClick={stop}><Square size={14}/></button>}</div>
      <small>{countLabel || '\u00a0'}</small>
      {active && progress && <code className="recursive-search-current" title={formatPath(progress.path)}>{formatPath(progress.path)}</code>}
      {error && <p role="alert" className="error-banner">{error}</p>}
      {result?.truncated && <p role="status" className="notice-banner">{text.partial}</p>}
      <div className="recursive-search-results" aria-label={text.title}>{result?.matches.map(item => <button type="button" key={item.path} title={`${text.reveal}: ${formatPath(item.path)}`} onClick={() => { setOpen(false); onNavigate(item.isDirectory ? item.path : item.path.slice(0, item.path.lastIndexOf('/')) || '/'); }}>
        {item.isDirectory ? <Folder size={18}/> : <File size={18}/>}<span><span className="recursive-search-name">{item.name}</span><small>{formatPath(item.path)}</small></span>
      </button>)}{result && !result.matches.length && !result.cancelled && <p>{text.empty}</p>}</div>
    </section></div>, document.body)}
  </>;
}
