/* 2D visualizations: block map (treemap), ring chart (sunburst), folder tree. */
(function () {
  const Lustro = (window.Lustro = window.Lustro || {});

  const deltaColor = (n) => {
    if (!n.d) return Lustro.css("--surface-3");
    const t = Math.min(1, Math.abs(n.d) / 5);
    return `color-mix(in srgb, ${Lustro.css(n.d > 0 ? "--grew" : "--shrank")} ${30 + t * 70}%, ${Lustro.css("--surface-3")})`;
  };

  /* Block map. opts: {root, selected, onSelect, colorBy: 'cat'|'delta', maxDepth, highlight(n)} */
  Lustro.treemap = function (el, opts) {
    const root = opts.root || Lustro.data.root;
    el.classList.add("treemap");
    el.innerHTML = "";
    const W = el.clientWidth, Hh = el.clientHeight;
    const maxDepth = opts.maxDepth || 4;
    const cells = [];
    (function draw(node, x, y, w, h, depth) {
      const kids = (node.children || []).filter((c) => c.size > 0);
      Lustro.squarify(kids, x, y, w, h).forEach((r) => {
        const n = r.n;
        const big = r.w > 70 && r.h > 40;
        if (n.children && depth < maxDepth && r.w > 18 && r.h > 18) {
          const head = big ? 16 : 0, pad = 2;
          if (big) {
            const lab = document.createElement("div");
            lab.className = "group-label";
            lab.style.cssText = `left:${r.x + 6}px;top:${r.y + 2}px;width:${r.w - 12}px`;
            lab.textContent = `${n.name} · ${Lustro.fmt(n.size)}`;
            el.appendChild(lab);
          }
          draw(n, r.x + pad, r.y + pad + head, r.w - pad * 2, r.h - pad * 2 - head, depth + 1);
        } else {
          const c = document.createElement("div");
          c.className = "cell" + (n.cloud ? " cloud" : "");
          c.style.cssText = `left:${r.x + 1}px;top:${r.y + 1}px;width:${Math.max(0, r.w - 2)}px;height:${Math.max(0, r.h - 2)}px;`;
          const cat = n.children ? Lustro.dominantCat(n) : n.cat;
          if (!n.cloud) c.style.background = opts.colorBy === "delta" ? deltaColor(n) : Lustro.catColor(cat);
          if (opts.colorBy === "delta" && !n.cloud) c.style.color = "var(--text)";
          if (r.w > 60 && r.h > 30) c.innerHTML = `<b>${n.name}</b><br><span class="mono">${opts.colorBy === "delta" && n.d ? Lustro.fmt(n.d, { sign: true }) : Lustro.fmt(n.size)}</span>`;
          if (n.provider && r.w > 34 && r.h > 18) c.insertAdjacentHTML("beforeend", Lustro.cloudMark(n, "corner"));
          c.onmousemove = (ev) => Lustro.tip(Lustro.nodeTip(n), ev);
          c.onmouseleave = () => Lustro.tip(null);
          c.onclick = () => opts.onSelect && opts.onSelect(n);
          c._node = n;
          el.appendChild(c);
          cells.push(c);
        }
      });
    })(root, 0, 0, W, Hh, 1);
    const api = {
      select(n) {
        cells.forEach((c) => {
          let inside = false; for (let x = c._node; x; x = x.parent) if (x === n) inside = true;
          c.classList.toggle("sel", c._node === n);
          c.classList.toggle("dim", !!n && n.children && !inside);
        });
      },
    };
    if (opts.selected) api.select(opts.selected);
    return api;
  };

  /* Ring chart (sunburst) in SVG. */
  Lustro.sunburst = function (el, opts) {
    const root = opts.root || Lustro.data.root;
    const W = el.clientWidth, H = el.clientHeight, R = Math.min(W, H) / 2 - 8, cx = W / 2, cy = H / 2;
    const levels = 4, r0 = R * 0.28, step = (R - r0) / levels;
    const arcs = [];
    (function walk(n, a0, a1, d) {
      if (!n.children || d > levels) return;
      let a = a0;
      n.children.forEach((c) => {
        const span = ((a1 - a0) * c.size) / n.size;
        arcs.push({ n: c, a0: a, a1: a + span, d });
        walk(c, a, a + span, d + 1);
        a += span;
      });
    })(root, 0, Math.PI * 2, 1);
    const p = (r, a) => [cx + r * Math.sin(a), cy - r * Math.cos(a)];
    const path = (ri, ro, a0, a1) => {
      const gap = Math.min(0.004, (a1 - a0) / 4);
      a0 += gap; a1 -= gap;
      const large = a1 - a0 > Math.PI ? 1 : 0;
      const [x0, y0] = p(ro, a0), [x1, y1] = p(ro, a1), [x2, y2] = p(ri, a1), [x3, y3] = p(ri, a0);
      return `M${x0},${y0}A${ro},${ro} 0 ${large} 1 ${x1},${y1}L${x2},${y2}A${ri},${ri} 0 ${large} 0 ${x3},${y3}Z`;
    };
    const svgNS = "http://www.w3.org/2000/svg";
    const svg = document.createElementNS(svgNS, "svg");
    svg.setAttribute("width", W); svg.setAttribute("height", H);
    el.innerHTML = ""; el.appendChild(svg);
    const els = [];
    arcs.forEach((a) => {
      if (a.a1 - a.a0 < 0.004) return;
      const e = document.createElementNS(svgNS, "path");
      e.setAttribute("d", path(r0 + (a.d - 1) * step + 1, r0 + a.d * step - 1, a.a0, a.a1));
      const cat = a.n.children ? Lustro.dominantCat(a.n) : a.n.cat;
      if (a.n.cloud) {
        e.setAttribute("fill", "none"); e.setAttribute("stroke", Lustro.catColor("cloud")); e.setAttribute("stroke-dasharray", "3 3");
      } else {
        e.setAttribute("fill", Lustro.catColor(cat));
        e.setAttribute("fill-opacity", a.n.children ? 0.45 + 0.12 * a.d : 0.95);
      }
      e.style.cursor = "pointer";
      e.onmousemove = (ev) => Lustro.tip(Lustro.nodeTip(a.n), ev);
      e.onmouseleave = () => Lustro.tip(null);
      e.onclick = () => opts.onSelect && opts.onSelect(a.n);
      e._node = a.n;
      svg.appendChild(e); els.push(e);
    });
    const label = document.createElementNS(svgNS, "text");
    label.setAttribute("x", cx); label.setAttribute("y", cy - 4); label.setAttribute("text-anchor", "middle");
    label.setAttribute("fill", Lustro.css("--text")); label.setAttribute("font-size", "22"); label.setAttribute("font-weight", "650");
    label.textContent = Lustro.fmt(root.size);
    const sub = document.createElementNS(svgNS, "text");
    sub.setAttribute("x", cx); sub.setAttribute("y", cy + 16); sub.setAttribute("text-anchor", "middle");
    sub.setAttribute("fill", Lustro.css("--text-3")); sub.setAttribute("font-size", "11");
    sub.textContent = root.name;
    svg.append(label, sub);
    return {
      select(n) {
        els.forEach((e) => {
          let inside = false; for (let x = e._node; x; x = x.parent) if (x === n) inside = true;
          let anc = false; for (let x = n; x; x = x.parent) if (x === e._node) anc = true;
          e.style.opacity = !n || inside || anc ? 1 : 0.25;
          e.setAttribute("stroke", e._node === n ? Lustro.css("--text") : e._node.cloud ? Lustro.catColor("cloud") : "none");
          if (e._node === n) e.setAttribute("stroke-width", 2);
        });
        label.textContent = Lustro.fmt((n || root).size);
        sub.textContent = (n || root).name;
      },
    };
  };

  /* Folder tree with expand/collapse. opts: {root, onSelect, open: Set, columns} */
  Lustro.folderTree = function (el, opts) {
    const root = opts.root || Lustro.data.root;
    const open = opts.open || new Set([root]);
    let selected = opts.selected || null;
    el.classList.add("tree");
    function render() {
      el.innerHTML = "";
      (function rows(n, depth) {
        (n.children || []).forEach((c) => {
          const r = document.createElement("div");
          r.className = "t-row" + (c === selected ? " sel" : "");
          const cat = c.children ? Lustro.dominantCat(c) : c.cat;
          const pct = (c.size / n.size) * 100;
          r.innerHTML = `
            <div class="t-name" style="padding-left:${depth * 14}px">
              <span class="caret ${open.has(c) ? "open" : ""}">${c.children ? "›" : ""}</span>
              <span class="dot" style="background:${c.cloud ? "transparent" : Lustro.catColor(cat)};${c.cloud ? "box-shadow:inset 0 0 0 1.5px var(--cat-cloud)" : ""}"></span>
              <span>${c.name}</span>${c.state ? Lustro.cloudMark(c, "mini") : ""}
            </div>
            <div class="pct"><i style="width:${pct}%;background:${Lustro.catColor(cat)}"></i></div>
            <div class="t-size">${Lustro.fmt(c.size)}</div>`;
          r.onclick = () => {
            if (c.children) { open.has(c) ? open.delete(c) : open.add(c); }
            selected = c; render(); opts.onSelect && opts.onSelect(c);
          };
          r.onmousemove = (ev) => Lustro.tip(Lustro.nodeTip(c), ev);
          r.onmouseleave = () => Lustro.tip(null);
          el.appendChild(r);
          if (c.children && open.has(c)) rows(c, depth + 1);
        });
      })(root, 0);
    }
    render();
    return {
      select(n) {
        selected = n;
        for (let x = n && n.parent; x; x = x.parent) open.add(x);
        render();
        const s = el.querySelector(".sel"); if (s) s.scrollIntoView({ block: "nearest" });
      },
      render,
    };
  };

  /* File tree as an icicle chart: one column per folder level, bar height = size. */
  Lustro.icicle = function (el, opts) {
    const root = opts.root || Lustro.data.root;
    el.classList.add("icicle"); el.innerHTML = "";
    const W = el.clientWidth, H = el.clientHeight, cols = 5, cw = W / cols;
    const bars = [];
    (function walk(n, y0, y1, d) {
      if (!n.children || d >= cols) return;
      let y = y0;
      n.children.forEach((c) => {
        const h = ((y1 - y0) * c.size) / n.size;
        if (h >= 1) {
          const b = document.createElement("div");
          b.className = "bar-n" + (c.cloud ? " cloud" : "");
          b.style.cssText = `left:${d * cw + 1}px;top:${y + 1}px;width:${cw - 2}px;height:${Math.max(1, h - 2)}px;`;
          if (!c.cloud) b.style.background = Lustro.catColor(c.children ? Lustro.dominantCat(c) : c.cat);
          if (c.children) b.style.opacity = 0.85;
          if (h > 18) b.innerHTML = `<b>${c.name}</b> <span class="mono">${Lustro.fmt(c.size)}</span> ${c.state ? Lustro.cloudMark(c) : ""}`;
          b.onmousemove = (ev) => Lustro.tip(Lustro.nodeTip(c), ev);
          b.onmouseleave = () => Lustro.tip(null);
          b.onclick = () => opts.onSelect(c);
          b._node = c; el.appendChild(b); bars.push(b);
        }
        walk(c, y, y + h, d + 1);
        y += h;
      });
    })(root, 0, H, 0);
    return {
      select(n) {
        bars.forEach((b) => {
          let inside = false; for (let x = b._node; x; x = x.parent) if (x === n) inside = true;
          let anc = false; for (let x = n; x; x = x.parent) if (x === b._node) anc = true;
          b.classList.toggle("sel", b._node === n);
          b.classList.toggle("dim", !!n && !inside && !anc);
        });
      },
    };
  };

  Lustro.legend = function (el, cats) {
    el.className = "legend";
    el.innerHTML = (cats || Object.keys(Lustro.CATS)).map((c) =>
      `<span><span class="dot" style="background:${Lustro.catColor(c)}"></span>${Lustro.CATS[c]}</span>`).join("") +
      `<span><span class="dot" style="box-shadow:inset 0 0 0 1.5px var(--cat-cloud)"></span>Cloud only</span>` +
      `<span>${Lustro.cloudMark({ provider: "onedrive", state: "downloaded" })} Downloaded</span>` +
      `<span>${Lustro.cloudMark({ provider: "onedrive", state: "pinned" })} Always on device</span>`;
  };
})();
