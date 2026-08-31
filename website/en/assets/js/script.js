const header = document.querySelector('[data-header]');
const menuToggle = document.querySelector('[data-menu-toggle]');
const nav = document.querySelector('[data-nav]');
const menuText = menuToggle?.querySelector('.sr-only');

const setHeaderState = () => {
  header?.classList.toggle('is-scrolled', window.scrollY > 18);
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
window.addEventListener('scroll', setHeaderState, { passive: true });
window.addEventListener('resize', () => {
  if (window.innerWidth > 760) closeMenu();
});
setHeaderState();

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
