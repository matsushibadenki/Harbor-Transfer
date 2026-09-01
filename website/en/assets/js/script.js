const header = document.querySelector('[data-header]');
const menuToggle = document.querySelector('[data-menu-toggle]');
const nav = document.querySelector('[data-nav]');
const menuText = menuToggle?.querySelector('.sr-only');
const sectionLinks = [...document.querySelectorAll('.site-nav > a[href^="#"]')];
const languageLinks = [...document.querySelectorAll('.language-switcher a')];

const setHeaderState = () => {
  header?.classList.toggle('is-scrolled', window.scrollY > 18);
};

const setActiveSection = () => {
  const marker = window.scrollY + window.innerHeight * 0.38;
  let activeId = '';
  sectionLinks.forEach((link) => {
    const section = document.querySelector(link.getAttribute('href'));
    if (section && section.offsetTop <= marker && section.offsetTop + section.offsetHeight > marker) {
      activeId = section.id;
    }
  });
  sectionLinks.forEach((link) => {
    if (link.getAttribute('href') === `#${activeId}`) link.setAttribute('aria-current', 'location');
    else link.removeAttribute('aria-current');
  });
};

const syncLanguageLinkHashes = () => {
  const sectionHash = sectionLinks.some((link) => link.getAttribute('href') === window.location.hash)
    ? window.location.hash
    : '';
  languageLinks.forEach((link) => {
    const target = new URL(link.href);
    target.hash = sectionHash;
    link.href = target.href;
  });
};

const setMenuState = (isOpen) => {
  menuToggle?.setAttribute('aria-expanded', String(isOpen));
  nav?.classList.toggle('is-open', isOpen);
  document.body.classList.toggle('menu-open', isOpen);
  if (menuText && menuToggle) {
    menuText.textContent = isOpen ? menuToggle.dataset.closeLabel : menuToggle.dataset.openLabel;
  }
};

const closeMenu = () => setMenuState(false);

menuToggle?.addEventListener('click', () => {
  const isOpen = menuToggle.getAttribute('aria-expanded') === 'true';
  setMenuState(!isOpen);
});

nav?.querySelectorAll('a').forEach((link) => link.addEventListener('click', closeMenu));
sectionLinks.forEach((link) => link.addEventListener('click', () => {
  sectionLinks.forEach((item) => item.removeAttribute('aria-current'));
  link.setAttribute('aria-current', 'location');
}));
window.addEventListener('hashchange', syncLanguageLinkHashes);
window.addEventListener('scroll', () => {
  setHeaderState();
  setActiveSection();
}, { passive: true });
window.addEventListener('resize', () => {
  if (window.innerWidth > 760) closeMenu();
  setActiveSection();
});
setHeaderState();
setActiveSection();
syncLanguageLinkHashes();

const revealItems = document.querySelectorAll('.reveal');
if ('IntersectionObserver' in window && !window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
  const observer = new IntersectionObserver((entries) => {
    entries.forEach((entry) => {
      if (entry.isIntersecting) {
        entry.target.classList.add('is-visible');
        observer.unobserve(entry.target);
      }
    });
  }, { threshold: 0.12, rootMargin: '0px 0px -40px' });
  revealItems.forEach((item) => observer.observe(item));
} else {
  revealItems.forEach((item) => item.classList.add('is-visible'));
}
