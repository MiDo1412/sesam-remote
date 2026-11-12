// Sesam Remote website: poster lift, door tabs, switches, counters.
const root = document.body;
root.classList.replace('no-js', 'js');
const still = matchMedia('(prefers-reduced-motion: reduce)');

// Poster: the app window drifts up over the title while scrolling
const win = document.querySelector('.hero .window');
if (win) {
  let ticking = false;
  const lift = () => {
    ticking = false;
    if (still.matches) return;
    const y = Math.min(scrollY, innerHeight);
    win.style.setProperty('--lift', (-y * .18).toFixed(1) + 'px');
  };
  addEventListener('scroll', () => { if (!ticking) { ticking = true; requestAnimationFrame(lift); } }, { passive: true });
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

// "Öffne Sesam auf …": das eigene System hervorheben (das iPad meldet sich wie ein Mac, hat aber Touch).
const ua = navigator.userAgent, touch = navigator.maxTouchPoints > 1;
const mine = /Windows/.test(ua) ? 'Windows' : /iPad|iPhone/.test(ua) || (/Macintosh/.test(ua) && touch) ? 'iPad' : /Macintosh/.test(ua) ? 'Mac' : /Linux/.test(ua) && !/Android/.test(ua) ? 'Linux' : 'Mac';
const pick = document.querySelector(`.pick a[data-os="${mine}"]`);
if (pick) pick.classList.add('on');
