// Show diagnostics only if the original dashboard actually failed to render.
(() => {
  let error = '';
  addEventListener('error', event => {error = event.message || 'A JavaScript asset failed to load';});
  addEventListener('unhandledrejection', event => {error = String(event.reason || 'JavaScript Promise failed');});
  addEventListener('DOMContentLoaded', () => {
    setTimeout(() => {
      const body = document.body;
      if (!body || document.getElementById('mics-boot-warning')) return;
      if (new URLSearchParams(location.search).get('legacy') === '1') return;
      const dashboard = location.pathname === '/' || location.pathname === '/index.html';
      const shell = body.querySelector('#sidebar');
      const view = body.querySelector('#view');
      const grid = body.querySelector('.machine-grid');
      const cards = grid ? grid.querySelectorAll('[data-machine]') : [];
      const selected = [...body.querySelectorAll('#view-seg button')]
        .find(el => el.classList.contains('bg-blue-600'));
      const inGanttOrTable = selected && !selected.textContent.includes('カード');
      const visible = body.querySelector('#app')?.getBoundingClientRect().height > 50;
      // The original dashboard legitimately has no cards in list or Gantt views.
      if (dashboard && shell && view && (cards.length > 0 || inGanttOrTable)) return;
      if (!dashboard && visible) return;
      if (dashboard && shell && view && view.textContent.includes('読み込みエラー')) return;
      const card = document.createElement('aside');
      card.id = 'mics-boot-warning';
      card.style.cssText = 'position:fixed;z-index:2147483647;top:18px;left:18px;right:18px;max-width:720px;background:#fff;color:#111;border:2px solid #b91c1c;padding:18px;border-radius:12px;box-shadow:0 8px 25px #0004;font:16px/1.5 sans-serif';
      const title = document.createElement('strong');
      title.textContent = 'MICS-only: Dashboard display error / 画面表示エラー';
      const detail = document.createElement('p');
      detail.textContent = (error || 'Machine node view did not render. Verify local JSON and browser Console.').slice(0,500);
      const link = document.createElement('a');
      link.href = '/?legacy=1';
      link.textContent = 'Open fallback dashboard / 旧画面を開く';
      link.style.cssText = 'color:#1d4ed8;text-decoration:underline';
      card.append(title,detail,link);
      body.prepend(card);
    }, 6500);
  });
})();
