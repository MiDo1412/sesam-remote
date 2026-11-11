// Sesam Remote website: keyhole scene, door tabs, switches, counters.
const root = document.body;
root.classList.replace('no-js', 'js');
const still = matchMedia('(prefers-reduced-motion: reduce)');

// Keyhole scene: the hole in the seed grows until the library fills the screen.
const opening = document.querySelector('.opening');
const door = opening && opening.querySelector('.door');
const hole = document.getElementById('kh');
const heroText = opening && opening.querySelector('.hero-text');
if (!door || still.matches) root.classList.add('no-scene');
let ticking = false;
function scene() {
  ticking = false;
  if (root.classList.contains('no-scene')) return;
  const box = opening.getBoundingClientRect();
  const run = box.height - innerHeight;
  const p = Math.min(1, Math.max(0, -box.top / run));
  const h = hole.getBoundingClientRect();
  const t = Math.min(1, Math.max(0, (p - .04) / .78));
  const e = t * t * (3 - 2 * t);
  // scale that makes the keyhole's narrow foot wider than the screen diagonal
  const max = 2.6 * Math.hypot(innerWidth, innerHeight) / h.width;
  const s = Math.pow(max, e);
  const w = h.width * s, hh = h.height * s;
  // move the hole's circle centre from the seed to the middle of the screen
  const cx = h.left + h.width / 2 + (innerWidth / 2 - h.left - h.width / 2) * e;
  const cy = h.top + h.height * .27 + (innerHeight / 2 - h.top - h.height * .27) * e;
  const st = door.style;
  st.setProperty('--kw', w + 'px');
  st.setProperty('--kh', hh + 'px');
  st.setProperty('--kx', (cx - w / 2) + 'px');
  st.setProperty('--ky', (cy - hh * .27) + 'px');
  st.setProperty('--zoom', (1.08 - .08 * e).toFixed(3));
  heroText.style.opacity = (1 - Math.min(1, t * 2.2)).toFixed(3);
  opening.style.setProperty('--say', Math.min(1, Math.max(0, (p - .8) / .12)).toFixed(3));
}
function ask() { if (!ticking) { ticking = true; requestAnimationFrame(scene); } }
if (door) {
  addEventListener('scroll', ask, { passive: true });
  addEventListener('resize', ask);
  still.addEventListener('change', () => { root.classList.toggle('no-scene', still.matches); ask(); });
  // the seed sways, so follow it while the scene is at rest
  (function loop() { scene(); if (!root.classList.contains('no-scene')) requestAnimationFrame(loop); })();
}

// Door tabs: Desktop, Shell, Dateien
const doors = document.querySelector('.doors');
if (doors) {
  const tabs = [...doors.querySelectorAll('[role=tab]')];
  const pics = [...doors.querySelectorAll('.shots picture')];
  const word = document.getElementById('door-word');
  const pick = (tab, focus) => {
    tabs.forEach((t, i) => {
      const on = t === tab;
      t.setAttribute('aria-selected', on);
      t.tabIndex = on ? 0 : -1;
      document.getElementById(t.getAttribute('aria-controls')).hidden = !on;
      pics[i] && pics[i].classList.toggle('on', on);
    });
    doors.dataset.door = tab.dataset.door;
    if (word) word.textContent = tab.dataset.word;
    if (focus) tab.focus();
  };
  tabs.forEach((t, i) => {
    t.addEventListener('click', () => pick(t));
    t.addEventListener('keydown', ev => {
      const k = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[ev.key];
      if (ev.key === 'Home') pick(tabs[0], true);
      else if (ev.key === 'End') pick(tabs[tabs.length - 1], true);
      else if (k) pick(tabs[(i + k + tabs.length) % tabs.length], true);
      else return;
      ev.preventDefault();
    });
  });
}

// Switches on the bands
document.querySelectorAll('.toggle').forEach(b => b.addEventListener('click', () => {
  const on = b.getAttribute('aria-checked') !== 'true';
  b.setAttribute('aria-checked', on);
  b.closest('.band').classList.toggle('on', on);
}));

// Reveal and port counters
const count = el => {
  const n = +el.dataset.n, t0 = performance.now();
  const step = now => {
    const k = Math.min(1, (now - t0) / 1100);
    el.textContent = Math.round(n * (1 - Math.pow(1 - k, 3)));
    if (k < 1) requestAnimationFrame(step);
  };
  requestAnimationFrame(step);
};
const seen = new IntersectionObserver(list => list.forEach(x => {
  if (!x.isIntersecting) return;
  x.target.classList.add('in');
  if (!still.matches) x.target.querySelectorAll('.port[data-n]').forEach(count);
  seen.unobserve(x.target);
}), { rootMargin: '0px 0px -8% 0px' });
document.querySelectorAll('.rise').forEach(el => seen.observe(el));

// "Öffne Sesam auf … jetzt."
const form = document.getElementById('get');
if (form) form.addEventListener('submit', ev => {
  ev.preventDefault();
  location.href = form.querySelector('select').value;
});
