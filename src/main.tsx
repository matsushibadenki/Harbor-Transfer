import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import App, { SshKeyManagerWindow } from './App';
import './styles.css';
import './phase2.css';

const systemDarkMode = window.matchMedia('(prefers-color-scheme: dark)');

function updateResolvedTheme() {
  const theme = document.documentElement.dataset.theme ?? 'system';
  document.documentElement.dataset.resolvedTheme = theme === 'system'
    ? (systemDarkMode.matches ? 'dark' : 'light')
    : theme;
}

try {
  const saved = JSON.parse(localStorage.getItem('harbor-transfer.preferences') ?? '{}') as { theme?: string };
  document.documentElement.dataset.theme = saved.theme ?? 'system';
} catch {
  document.documentElement.dataset.theme = 'system';
}

updateResolvedTheme();
new MutationObserver(updateResolvedTheme).observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
systemDarkMode.addEventListener('change', updateResolvedTheme);

const isKeyManagerWindow = window.location.hash === '#ssh-keys';
document.body.classList.toggle('key-manager-body', isKeyManagerWindow);

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    {isKeyManagerWindow ? <SshKeyManagerWindow /> : <App />}
  </StrictMode>,
);
