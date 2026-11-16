// Lightweight boot diagnostics: only displays a notice if the page fails to render.
(() => {
  let error = '';
  addEventListener('error', event => { error = event.message || 'JavaScript or asset failed to load'; });
  addEventListener('unhandledrejection', event => { error = String(event.reason || 'JavaScript Promise failed'); });
  addEventListener('DOMContentLoaded', () => {
    setTimeout(() => {
      const body = document.body;
      if (!body || document.getElementById('mics-boot-warning')) return;
      const content = (body.innerText || '').trim();
      const visible = [...body.querySelectorAll('main, #app, [id*="app"], [class*="dashboard"]')]
        .some(el => el.children.length > 2 && el.getBoundingClientRect().height > 50);
      if (visible || content.length > 100) return;
      const card = document.createElement('aside');
      card.id = 'mics-boot-warning';
      card.style.cssText = 'position:fixed;z-index:2147483647;top:18px;left:18px;right:18px;max-width:720px;background:#fff;color:#111;border:2px solid #b91c1c;padding:18px;border-radius:12px;box-shadow:0 8px 25px #0004;font:16px/1.5 sans-serif';
      const title = document.createElement('strong');
      title.textContent = 'MICS-only: JavaScript template failed to render / 画面表示エラー';
      const detail = document.createElement('p');
      detail.textContent = (error || 'The page did not render. Check browser Console and network asset requests.').slice(0,500);
      const link = document.createElement('a');
      link.href = '/?legacy=1';
      link.textContent = 'Open fallback dashboard / 旧画面を開く';
      link.style.cssText = 'color:#1d4ed8;text-decoration:underline';
      card.append(title,detail,link);
      body.prepend(card);
    }, 6500);
  });
})();
