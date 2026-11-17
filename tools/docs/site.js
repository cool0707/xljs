// Search over the pages, headings and API entries (search.js), the menu on
// narrow screens and the current heading in the outline.
(() => {
  const base = window.XLJS_BASE || ".";
  const index = window.XLJS_SEARCH || [];
  const input = document.querySelector(".search input");
  const list = document.querySelector(".search .results");
  let sel = -1;

  const score = (t, q) => {
    const l = t.toLowerCase();
    if (l === q) return 0;
    if (l.startsWith(q)) return 1;
    const i = l.indexOf(q);
    if (i >= 0) return l[i - 1] === "." ? 2 : 3;
    return -1;
  };
  const show = () => {
    const q = input.value.trim().toLowerCase();
    if (!q) return (list.hidden = true);
    const hits = index
      .map((e) => [score(e.t, q), e])
      .filter(([s]) => s >= 0)
      .sort((a, b) => a[0] - b[0] || a[1].t.length - b[1].t.length)
      .slice(0, 40);
    list.replaceChildren(
      ...hits.map(([, e], i) => {
        const li = document.createElement("li");
        const a = document.createElement("a");
        a.href = base + "/" + e.u;
        a.textContent = e.t;
        const k = document.createElement("span");
        k.textContent = e.k;
        a.append(k);
        li.append(a);
        if (i === 0) li.className = "sel";
        return li;
      }),
    );
    sel = hits.length ? 0 : -1;
    list.hidden = !hits.length;
  };
  const move = (d) => {
    const items = list.querySelectorAll("li");
    if (!items.length) return;
    items[sel]?.classList.remove("sel");
    sel = (sel + d + items.length) % items.length;
    items[sel].classList.add("sel");
    items[sel].scrollIntoView({ block: "nearest" });
  };
  input.addEventListener("input", show);
  input.addEventListener("focus", show);
  input.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") move(1), e.preventDefault();
    else if (e.key === "ArrowUp") move(-1), e.preventDefault();
    else if (e.key === "Enter") list.querySelectorAll("li a")[sel]?.click();
    else if (e.key === "Escape") (input.value = ""), (list.hidden = true), input.blur();
  });
  document.addEventListener("click", (e) => {
    if (!e.target.closest(".search")) list.hidden = true;
  });
  document.addEventListener("keydown", (e) => {
    if (e.key === "/" && document.activeElement !== input) e.preventDefault(), input.focus();
  });

  const menu = document.querySelector(".menu");
  menu.addEventListener("click", () => {
    const open = document.body.classList.toggle("nav-open");
    menu.setAttribute("aria-expanded", open);
  });

  // The top page: the tabs of the pictures, and the development mode to try
  // (demo/web/dev-demo.html at its own size, scaled to the width).
  for (const g of document.querySelectorAll(".gallery")) {
    const tabs = [...g.querySelectorAll(".tabs button")];
    const panels = [...g.querySelectorAll("figure")];
    tabs.forEach((t, i) => t.addEventListener("click", () => {
      tabs.forEach((x, k) => x.setAttribute("aria-selected", String(k === i)));
      panels.forEach((p, k) => (p.hidden = k !== i));
    }));
  }
  for (const d of document.querySelectorAll(".demo[data-demo-page]")) {
    const box = d.querySelector(".demo-frame");
    const start = () => {
      if (box.querySelector("iframe")) return;
      const f = document.createElement("iframe");
      f.src = base + "/" + d.dataset.demoPage;
      f.title = "開発モードの体験版";
      box.replaceChildren(f);
      const fit = () => (f.style.transform = `scale(${box.clientWidth / 1280})`);
      fit();
      new ResizeObserver(fit).observe(box);
    };
    d.querySelector(".demo-start").addEventListener("click", start);
    // Opened by itself when it comes into view on a wide screen.
    if ("IntersectionObserver" in window && matchMedia("(min-width: 900px)").matches) {
      const io = new IntersectionObserver((es) => { if (es.some((e) => e.isIntersecting)) { io.disconnect(); start(); } });
      io.observe(box);
    }
  }

  const links = [...document.querySelectorAll("nav.toc a")];
  if (links.length && "IntersectionObserver" in window) {
    const byId = new Map(links.map((a) => [decodeURIComponent(a.hash.slice(1)), a]));
    const io = new IntersectionObserver((entries) => {
      for (const e of entries) {
        if (!e.isIntersecting) continue;
        links.forEach((a) => a.classList.remove("active"));
        byId.get(e.target.id)?.classList.add("active");
      }
    }, { rootMargin: "-60px 0px -70% 0px" });
    byId.forEach((_, id) => {
      const el = document.getElementById(id);
      if (el) io.observe(el);
    });
  }
})();
