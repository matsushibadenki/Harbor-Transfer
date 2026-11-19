import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { createContext, lazy, Suspense, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from 'react';
export const quickLookLabels = { ja: 'クイックルック (Space)', en: 'Quick Look (Space)', 'zh-CN': '快速查看 (Space)' };
export type PreviewEndpoint = { kind: 'local'; path: string } | { kind: 'remote'; path: string; connectionId: string };
export type PreviewStatus = { language: 'ja' | 'en' | 'zh-CN'; name: string; phase: 'preparing' | 'open' | 'stopping' | 'error'; error?: string };
const Toast = lazy(() => import('./quick-look-toast'));
const Context = createContext<(endpoint: PreviewEndpoint, language: PreviewStatus['language'], name: string) => void>(() => {});
type Operation = { id: string; started: boolean; cancelled: boolean; unlisten?: () => void };
export function QuickLookProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<PreviewStatus | null>(null);
  const current = useRef<Operation | null>(null);
  const mounted = useRef(false);
  function close() {
    const job = current.current;
    if (!job) { setStatus(null); return; }
    if (job.cancelled) return;
    job.cancelled = true;
    setStatus(value => value ? { ...value, phase: 'stopping' } : value);
    if (job.started) void invoke('quick_look_close', { previewId: job.id }).catch(error => {
      if (mounted.current) setStatus(value => value ? { ...value, error: String(error) } : value);
    });
  }
  useEffect(() => {
    mounted.current = true;
    const escape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || !current.current || document.querySelector('[role="dialog"], [role="menu"]') || (event.target as HTMLElement).closest('input, textarea, select')) return;
      event.preventDefault(); close();
    };
    window.addEventListener('keydown', escape);
    return () => {
      mounted.current = false; window.removeEventListener('keydown', escape);
      const job = current.current;
      if (job) { job.cancelled = true; job.unlisten?.(); if (job.started) void invoke('quick_look_close', { previewId: job.id }).catch(() => {}); }
    };
  }, []);
  const preview = useCallback(async (endpoint: PreviewEndpoint, language: PreviewStatus['language'], name: string) => {
    if (current.current) { close(); return; }
    const job: Operation = { id: crypto.randomUUID(), cancelled: false, started: false };
    current.current = job; setStatus({ language, name, phase: 'preparing' });
    try {
      job.unlisten = await getCurrentWebviewWindow().listen<string>('preview://opened', event => {
        if (mounted.current && current.current === job && !job.cancelled && event.payload === job.id) setStatus(value => value ? { ...value, phase: 'open' } : value);
      });
      if (!mounted.current || job.cancelled) { if (mounted.current) setStatus(null); return; }
      job.started = true;
      await invoke('quick_look', { endpoint, previewId: job.id });
      if (mounted.current && current.current === job) setStatus(null);
    } catch (error) {
      if (mounted.current && current.current === job) setStatus({ language, name, phase: 'error', error: String(error) });
    } finally {
      job.unlisten?.();
      if (current.current === job) current.current = null;
    }
  }, []);
  return <Context.Provider value={preview}>{children}{status && <Suspense fallback={null}><Toast status={status} onClose={close}/></Suspense>}</Context.Provider>;
}
export function useQuickLook() { return useContext(Context); }
