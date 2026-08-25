import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import App, { SshKeyManagerWindow } from './App';
import './styles.css';
import './phase2.css';

try {
  const saved = JSON.parse(localStorage.getItem('harbor-transfer.preferences') ?? '{}') as { theme?: string };
  document.documentElement.dataset.theme = saved.theme ?? 'system';
} catch {
  document.documentElement.dataset.theme = 'system';
}

const isKeyManagerWindow = window.location.hash === '#ssh-keys';
document.body.classList.toggle('key-manager-body', isKeyManagerWindow);

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    {isKeyManagerWindow ? <SshKeyManagerWindow /> : <App />}
  </StrictMode>,
);
