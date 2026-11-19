import { AlertCircle, Eye, Info, LoaderCircle, X } from 'lucide-react';
import type { PreviewStatus } from './quick-look';
const copy = {
  ja: { preparing: 'プレビューを準備中…', open: 'Quick Lookを表示中', stopping: 'プレビューの終了待ち…', error: 'プレビューできませんでした', close: 'プレビューを閉じる', hint: 'Spaceで開閉。通常ファイルを専用キャッシュで表示し、終了時に削除します。上限64 MiB。準備中の中止は通信の完了を待ちます。' },
  en: { preparing: 'Preparing preview…', open: 'Quick Look is open', stopping: 'Closing preview…', error: 'Could not preview file', close: 'Close preview', hint: 'Press Space to toggle. Regular files are shown from a private cache, removed when closed. Limit: 64 MiB. Cancelling preparation waits for the current transfer.' },
  'zh-CN': { preparing: '正在准备预览…', open: 'Quick Look预览中', stopping: '正在关闭预览…', error: '无法预览文件', close: '关闭预览', hint: '按空格键开关预览。普通文件使用独立缓存，关闭后删除。上限64 MiB。准备期间取消会等待当前传输完成。' },
};
export default function QuickLookToast({ status, onClose }: { status: PreviewStatus; onClose: () => void }) {
  const text = copy[status.language];
  return <aside className="quick-look-status" role="status" aria-live="polite">
    {status.phase === 'preparing' || status.phase === 'stopping' ? <LoaderCircle size={18} className="spinning"/> : status.phase === 'error' ? <AlertCircle size={18}/> : <Eye size={18}/>}
    <div><span>{text[status.phase]}</span><span className="quick-look-help" title={text.hint} aria-label={text.hint} tabIndex={0}><Info size={12}/></span><small title={status.name}>{status.name}</small>{status.error && <small className="quick-look-error" role="alert">{status.error}</small>}</div>
    <button type="button" className="icon-button" disabled={status.phase === 'stopping'} title={text.close} aria-label={text.close} onClick={onClose}><X size={16}/></button>
  </aside>;
}
