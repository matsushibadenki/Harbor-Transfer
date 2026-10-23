import { Plus, X, Cloud, HardDrive } from 'lucide-react';
import { useEffect, useState } from 'react';

export type TabPane = { kind: 'local' | 'remote'; bookmarkId: string | null; path: string };
export type WorkspaceTab = {
  id: string; name: string; primary: TabPane; auxiliary: TabPane;
  dualPane: boolean; leftPaneKind: 'local' | 'remote'; ratio: number;
  viewMode: 'list' | 'icons' | 'columns'; widths: Record<'name' | 'size' | 'modified' | 'permissions' | 'owner' | 'group' | 'type', number>;
  visibility: Record<'size' | 'modified' | 'permissions' | 'owner' | 'group' | 'type', boolean>;
  sortColumn: keyof WorkspaceTab['widths']; sortDirection: 'ascending' | 'descending';
};
export type WorkspaceDocument = { version: 1; activeId: string; tabs: WorkspaceTab[] };
export const tabsCopy = {
  ja: { tabs: 'ワークスペースのタブ', add: '新規タブ', close: 'タブを閉じる', rename: 'タブ名を変更', unnamed: '新規タブ', reconnect: '再接続', offline: '未接続', closeBusy: '転送または編集中のファイルがあります。タブを閉じても処理は継続します。', keep: '戻る', confirm: 'タブを閉じる', restoreError: 'ワークスペースを復元できませんでした', retry: '再試行' },
  en: { tabs: 'Workspace tabs', add: 'New tab', close: 'Close tab', rename: 'Rename tab', unnamed: 'New tab', reconnect: 'Reconnect', offline: 'Disconnected', closeBusy: 'Transfers or editor sessions are active. They will continue after this tab closes.', keep: 'Back', confirm: 'Close tab', restoreError: 'Could not restore the workspace', retry: 'Retry' },
  'zh-CN': { tabs: '工作区标签页', add: '新建标签页', close: '关闭标签页', rename: '重命名标签页', unnamed: '新建标签页', reconnect: '重新连接', offline: '未连接', closeBusy: '传输或编辑会话仍在进行。关闭标签页后将继续处理。', keep: '返回', confirm: '关闭标签页', restoreError: '无法恢复工作区', retry: '重试' },
};
export function WorkspaceTabs({ tabs, activeId, language, label, connected, onSelect, onAdd, onClose, onRename }: {
  tabs: WorkspaceTab[]; activeId: string; language: keyof typeof tabsCopy;
  label: (tab: WorkspaceTab) => string; connected: (tab: WorkspaceTab) => boolean;
  onSelect: (id: string) => void; onAdd: () => void; onClose: (id: string) => void; onRename: (id: string, name: string) => void;
}) {
  const text = tabsCopy[language];
  const [editing, setEditing] = useState<string | null>(null);
  const [draft, setDraft] = useState('');
  useEffect(() => {
    document.getElementById(`workspace-tab-${activeId}`)?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  }, [activeId]);
  return <div className="workspace-tab-strip"><div role="tablist" aria-label={text.tabs} onKeyDown={(event) => {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key) || event.target instanceof HTMLInputElement) return;
    event.preventDefault();
    const index = tabs.findIndex(tab => tab.id === activeId);
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? tabs.length - 1 : (index + (event.key === 'ArrowRight' ? 1 : -1) + tabs.length) % tabs.length;
    onSelect(tabs[next].id);
    requestAnimationFrame(() => document.getElementById(`workspace-tab-${tabs[next].id}`)?.focus());
  }}>{tabs.map(tab => <div className={`workspace-tab ${tab.id === activeId ? 'selected' : ''}`} key={tab.id}>
    <button id={`workspace-tab-${tab.id}`} role="tab" aria-controls="workspace-panel" aria-selected={tab.id === activeId} tabIndex={tab.id === activeId ? 0 : -1} title={`${label(tab)} · ${text.rename}${connected(tab) ? '' : ` · ${text.offline}`}`} onClick={() => onSelect(tab.id)} onDoubleClick={() => { setDraft(tab.name || label(tab)); setEditing(tab.id); }}>
      {tab.primary.kind === 'local' ? <HardDrive size={13}/> : <Cloud size={13}/>}<span>{label(tab)}</span><i className={connected(tab) ? 'connected' : ''}/>
    </button>
    {editing === tab.id && <input className="tab-rename" autoFocus onFocus={event => event.target.select()} aria-label={text.rename} title={text.rename} maxLength={80} value={draft} onChange={event => setDraft(event.target.value.replace(/[\u0000-\u001f\u007f]/g, ''))} onBlur={() => { onRename(tab.id, draft.trim()); setEditing(null); }} onKeyDown={event => { if (event.key === 'Enter') { event.currentTarget.blur(); } if (event.key === 'Escape') { setEditing(null); } event.stopPropagation(); }}/>}
    <button className="tab-close" aria-label={`${text.close}: ${label(tab)}`} onClick={() => onClose(tab.id)}><X size={12}/></button>
  </div>)}</div><button className="icon-button" title={`${text.add} (⌘T)`} aria-label={text.add} disabled={tabs.length >= 32} onClick={onAdd}><Plus size={15}/></button></div>;
}
