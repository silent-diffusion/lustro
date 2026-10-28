/* "Terrain of your computer": a 3D contour map built from the folder tree.
   Ground layout  = squarified treemap, warped from a square onto a disc.
   Elevation      = cushion surface: every folder adds a rounded hill over its patch,
                    so heavy, deeply nested folders become tall peaks.
   Cloud-only     = valleys below ground level (they take no space on this disk). Downloaded and
                    pinned cloud files are ordinary land: they do use local space.
   Classic script: three.js is passed in from a module so the page also works from file://. */
(function () {
  const Lustro = (window.Lustro = window.Lustro || {});

  Lustro.mountTerrain = function (el, THREE, OrbitControls, opts) {
    opts = Object.assign({ tint: true, pins: { big: true, grew: true, dup: true, cloud: true }, autoRotate: false, compact: false }, opts);
    const root = opts.root || Lustro.data.root;
    THREE.ColorManagement.enabled = false;

    const N = opts.compact ? 150 : 210;
    const MAXH = 0.62;

    // ---------- Layout (square coords in [-1, 1]) ----------
    const rects = new Map(); // node -> {x0,y0,x1,y1,d}
    (function lay(node, x0, y0, x1, y1, d) {
      rects.set(node, { x0, y0, x1, y1, d });
      if (!node.children) return;
      const pad = d === 0 ? 0 : Math.min(x1 - x0, y1 - y0) * 0.04;
      const kids = node.children.filter((c) => c.size > 0);
      Lustro.squarify(kids, x0 + pad, y0 + pad, x1 - x0 - 2 * pad, y1 - y0 - 2 * pad).forEach((r) =>
        lay(r.n, r.x, r.y, r.x + r.w, r.y + r.h, d + 1));
    })(root, -1, -1, 1, 1, 0);
    const leafRects = [...rects].filter(([n]) => !n.children);

    const toDisc = (sx, sy) => [sx * Math.sqrt(1 - (sy * sy) / 2), sy * Math.sqrt(1 - (sx * sx) / 2)];
    const leafAt = (sx, sy) => { for (const [n, r] of leafRects) if (sx >= r.x0 && sx <= r.x1 && sy >= r.y0 && sy <= r.y1) return n; return null; };

    // ---------- Heightfield ----------
    const H = new Float32Array(N * N);
    const cell = 2 / (N - 1);
    const idx = (i, j) => j * N + i;
    rects.forEach((r, node) => {
      if (r.d === 0) return;
      if (!node.children && node.cloud) return; // carved as a lake below
      const w = r.x1 - r.x0, h = r.y1 - r.y0;
      const onDiskShare = node.children ? node.onDisk / node.size : 1;
      const amp = Math.sqrt(w * h) * onDiskShare * (node.children ? 1 : 0.8);
      const i0 = Math.max(0, Math.ceil((r.x0 + 1) / cell)), i1 = Math.min(N - 1, Math.floor((r.x1 + 1) / cell));
      const j0 = Math.max(0, Math.ceil((r.y0 + 1) / cell)), j1 = Math.min(N - 1, Math.floor((r.y1 + 1) / cell));
      for (let j = j0; j <= j1; j++) {
        const t = ((j * cell - 1) - r.y0) / h, by = Math.pow(Math.max(0, 4 * t * (1 - t)), 0.65);
        for (let i = i0; i <= i1; i++) {
          const s = ((i * cell - 1) - r.x0) / w, bx = Math.pow(Math.max(0, 4 * s * (1 - s)), 0.65);
          H[idx(i, j)] += amp * bx * by;
        }
      }
    });
    // Cloud-only files become valleys. A blurred mask lowers the surrounding land to ground level
    // over a wide band, so the slopes read like hillsides, then the valley floor sinks below
    // ground. Depth shows how much lives only in the cloud.
    const ease = (x) => { x = Math.min(1, Math.max(0, x)); return x * x * (3 - 2 * x); };
    const M = new Float32Array(N * N), floor = new Float32Array(N * N);
    const RIM = 0.15;
    rects.forEach((r, node) => {
      if (node.children || !node.cloud) return;
      const w = r.x1 - r.x0, h = r.y1 - r.y0, amp = Math.sqrt(w * h) * 2.2;
      const i0 = Math.max(0, Math.ceil((r.x0 + 1) / cell)), i1 = Math.min(N - 1, Math.floor((r.x1 + 1) / cell));
      const j0 = Math.max(0, Math.ceil((r.y0 + 1) / cell)), j1 = Math.min(N - 1, Math.floor((r.y1 + 1) / cell));
      for (let j = j0; j <= j1; j++) {
        const t = ((j * cell - 1) - r.y0) / h, ti = ease((t - RIM) / (1 - 2 * RIM));
        for (let i = i0; i <= i1; i++) {
          const s = ((i * cell - 1) - r.x0) / w, si = ease((s - RIM) / (1 - 2 * RIM));
          M[idx(i, j)] = 1;
          floor[idx(i, j)] = amp * Math.pow(Math.sin(Math.PI * ti) * Math.sin(Math.PI * si), 0.9);
        }
      }
    });
    const blur = (A, rad) => {
      const B = new Float32Array(N * N);
      for (let j = 0; j < N; j++) for (let i = 0; i < N; i++) {
        let sum = 0, n = 0;
        for (let k = -rad; k <= rad; k++) { const x = i + k; if (x >= 0 && x < N) { sum += A[idx(x, j)]; n++; } }
        B[idx(i, j)] = sum / n;
      }
      for (let j = 0; j < N; j++) for (let i = 0; i < N; i++) {
        let sum = 0, n = 0;
        for (let k = -rad; k <= rad; k++) { const y = j + k; if (y >= 0 && y < N) { sum += B[idx(i, y)]; n++; } }
        A[idx(i, j)] = sum / n;
      }
    };
    const Mb = M.slice(); blur(Mb, Math.round(N / 28)); blur(Mb, Math.round(N / 28));
    for (let k = 0; k < H.length; k++) {
      const lower = Math.max(M[k], ease(Mb[k] * 2.2));
      H[k] = H[k] * (1 - lower) - floor[k];
    }
    // Soften creases between siblings so it reads as terrain, not boxes.
    for (let pass = 0; pass < 3; pass++) {
      const C = H.slice();
      for (let j = 1; j < N - 1; j++) for (let i = 1; i < N - 1; i++) {
        H[idx(i, j)] = (C[idx(i, j)] * 4 + C[idx(i - 1, j)] + C[idx(i + 1, j)] + C[idx(i, j - 1)] + C[idx(i, j + 1)]) / 8;
      }
    }
    let hmax = 0; for (let k = 0; k < H.length; k++) hmax = Math.max(hmax, H[k]);
    for (let k = 0; k < H.length; k++) H[k] = Math.sign(H[k]) * Math.pow(Math.abs(H[k]) / hmax, 0.7) * MAXH;
    // Foothills: ease the rim down to ground level and lift the middle slightly, so the
    // whole drive reads as one massif. The same factor applies everywhere at a given radius,
    // so neighbours still compare honestly.
    for (let j = 0; j < N; j++) for (let i = 0; i < N; i++) {
      const sx = i * cell - 1, sy = j * cell - 1;
      const e = Math.min(1, Math.min(1 - Math.abs(sx), 1 - Math.abs(sy)) / 0.3);
      const [dx, dz] = toDisc(sx, sy), r2 = dx * dx + dz * dz;
      H[idx(i, j)] *= e * e * (3 - 2 * e) * (0.6 + 0.4 * (1 - r2));
    }
    const heightAt = (sx, sy) => {
      const fx = Math.min(N - 1.001, Math.max(0, (sx + 1) / cell)), fy = Math.min(N - 1.001, Math.max(0, (sy + 1) / cell));
      const i = Math.floor(fx), j = Math.floor(fy), tx = fx - i, ty = fy - j;
      return (H[idx(i, j)] * (1 - tx) + H[idx(i + 1, j)] * tx) * (1 - ty) + (H[idx(i, j + 1)] * (1 - tx) + H[idx(i + 1, j + 1)] * tx) * ty;
    };

    // ---------- Scene ----------
    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(30, 1, 0.1, 50);
    camera.position.set(0, 2.55, 3.55);
    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    renderer.outputColorSpace = THREE.LinearSRGBColorSpace;
    renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
    el.innerHTML = "";
    el.appendChild(renderer.domElement);
    const overlay = document.createElement("div");
    overlay.style.cssText = "position:absolute;inset:0;pointer-events:none";
    el.appendChild(overlay);

    const controls = new OrbitControls(camera, renderer.domElement);
    controls.target.set(0, 0.12, 0);
    controls.enableDamping = true;
    controls.minDistance = 1.0; controls.maxDistance = 6.5;
    controls.minPolarAngle = 0.01; controls.maxPolarAngle = Math.PI - 0.12;
    controls.enablePan = false;
    controls.autoRotate = !!opts.autoRotate; controls.autoRotateSpeed = 0.5;
    controls.addEventListener("start", () => (controls.autoRotate = false));

    const col = (v) => new THREE.Color(Lustro.css(v) || "#888");

    // Terrain geometry
    const pos = new Float32Array(N * N * 3), sq = new Float32Array(N * N * 2), tint = new Float32Array(N * N * 3), cloud = new Float32Array(N * N);
    const catColors = {};
    const fillTint = () => {
      Object.keys(Lustro.CATS).forEach((c) => (catColors[c] = col("--cat-" + c)));
      for (let j = 0; j < N; j++) for (let i = 0; i < N; i++) {
        const k = idx(i, j), sx = i * cell - 1, sy = j * cell - 1, leaf = leafAt(sx, sy);
        const c = leaf ? catColors[leaf.cat] : catColors.other;
        tint[k * 3] = c.r; tint[k * 3 + 1] = c.g; tint[k * 3 + 2] = c.b;
        cloud[k] = leaf && leaf.cloud ? 1 : 0;
      }
    };
    fillTint();
    for (let j = 0; j < N; j++) for (let i = 0; i < N; i++) {
      const k = idx(i, j), sx = i * cell - 1, sy = j * cell - 1, [dx, dz] = toDisc(sx, sy);
      pos[k * 3] = dx; pos[k * 3 + 1] = H[k]; pos[k * 3 + 2] = -dz;
      sq[k * 2] = sx; sq[k * 2 + 1] = sy;
    }
    const index = [];
    for (let j = 0; j < N - 1; j++) for (let i = 0; i < N - 1; i++) {
      const a = idx(i, j), b = idx(i + 1, j), c = idx(i, j + 1), d = idx(i + 1, j + 1);
      index.push(a, c, b, b, c, d);
    }
    const geo = new THREE.BufferGeometry();
    geo.setAttribute("position", new THREE.BufferAttribute(pos, 3));
    geo.setAttribute("sq", new THREE.BufferAttribute(sq, 2));
    geo.setAttribute("tint", new THREE.BufferAttribute(tint, 3));
    geo.setAttribute("cloud", new THREE.BufferAttribute(cloud, 1));
    geo.setIndex(index);
    geo.computeBoundingSphere();

    const uniforms = {
      uFill: { value: col("--terrain-fill") }, uLine: { value: col("--terrain-line") }, uMinor: { value: col("--terrain-line-minor") },
      uAccent: { value: col("--accent") }, uCloud: { value: col("--cat-cloud") }, uGrew: { value: col("--grew") },
      uInterval: { value: MAXH / 30 }, uTint: { value: opts.tint ? 1 : 0 },
      uSel: { value: new THREE.Vector4(9, 9, 9, 9) }, uHover: { value: new THREE.Vector4(9, 9, 9, 9) },
      uLight: { value: document.documentElement.dataset.theme === "light" ? 1 : 0 },
    };
    // FLOOR builds the underside: the same mesh pressed flat to ground level, shown only from below.
    const vert = (floor) => `
        attribute vec2 sq; attribute vec3 tint; attribute float cloud;
        varying float vH; varying vec2 vSq; varying vec3 vTint; varying float vCloud; varying vec3 vPos;
        void main() {
          vH = position.y; vSq = sq; vTint = tint; vCloud = cloud;
          vec3 p = position; ${floor ? "p.y = -0.002;" : ""}
          vec4 wp = modelMatrix * vec4(p, 1.0); vPos = wp.xyz;
          gl_Position = projectionMatrix * viewMatrix * wp;
        }`;
    const frag = (floor) => `
        uniform vec3 uFill, uLine, uMinor, uAccent, uCloud;
        uniform float uInterval, uTint, uLight;
        uniform vec4 uSel, uHover;
        varying float vH; varying vec2 vSq; varying vec3 vTint; varying float vCloud; varying vec3 vPos;
        float contour(float v, float w) { float f = abs(fract(v - 0.5) - 0.5) / (fwidth(v) * w); return 1.0 - clamp(f, 0.0, 1.0); }
        bool inside(vec4 r) { return vSq.x > r.x && vSq.x < r.z && vSq.y > r.y && vSq.y < r.w; }
        void main() {
          vec3 n = normalize(cross(dFdx(vPos), dFdy(vPos)));
          float light = clamp(dot(n, normalize(vec3(-0.5, 1.0, 0.35))), 0.0, 1.0);
          ${floor ? `
          // Underside: only from below, and not inside the valleys. Shade by the height above, so each
          // mountain's footprint shows through in its own color, a little darker than the top.
          if (cameraPosition.y > 0.0 || vH < 0.0) discard;
          light = 0.15 + 0.55 * clamp(vH / (uInterval * 30.0), 0.0, 1.0);` : ""}
          vec3 base = uFill * mix(0.7, 1.15, light);
          base = mix(base, vTint * mix(0.55, 1.0, light), uTint * mix(0.22, 0.3, uLight));
          float h = vH / uInterval;
          float onLand = smoothstep(0.25, 0.9, abs(h));
          float minor = contour(h, 1.0) * onLand;
          float major = contour(h / 5.0, 1.6) * onLand;
          ${floor ? "major = 0.0; minor *= 0.3; // underside: faint grey rings only, so the valleys stand out" : ""}
          vec3 c = mix(base, uMinor, minor * 0.85);
          c = mix(c, uLine, major * 0.9);
          if (inside(uSel)) {
            c = mix(c, uAccent, 0.10);
            c = mix(c, uAccent, max(minor, major) * 0.9);
          }
          if (inside(uHover)) c = mix(c, uLine, 0.07);
          // Valleys: water tint deepens with depth; the ground-level shoreline is drawn in the cloud color.
          float depth = clamp(-vH / 0.2, 0.0, 1.0);
          float below = step(vH, 0.0) * vCloud;
          c = mix(c, uCloud, below * (0.22 + depth * 0.5));
          c = mix(c, uCloud, below * max(minor, major) * 0.6);
          float shore = 1.0 - clamp(abs(vH) / (fwidth(vH) * 1.8), 0.0, 1.0);
          c = mix(c, uCloud, shore * vCloud);
          ${floor ? "c *= 0.85;" : ""}
          gl_FragColor = vec4(c, 1.0);
        }`;
    const mat = new THREE.ShaderMaterial({ uniforms, vertexShader: vert(false), fragmentShader: frag(false) });
    mat.side = THREE.DoubleSide;
    const terrain = new THREE.Mesh(geo, mat);
    scene.add(terrain);
    const floorMat = new THREE.ShaderMaterial({ uniforms, vertexShader: vert(true), fragmentShader: frag(true), side: THREE.DoubleSide });
    scene.add(new THREE.Mesh(geo, floorMat));

    // Base disc and compass ring
    const baseMat = new THREE.MeshBasicMaterial({ color: col("--terrain-fill") });
    const base = new THREE.Mesh(new THREE.RingGeometry(0.995, 1.2, 128), baseMat);
    baseMat.side = THREE.DoubleSide;
    base.rotation.x = -Math.PI / 2; base.position.y = -0.004; scene.add(base);
    const ringMat = new THREE.MeshBasicMaterial({ color: col("--accent"), side: THREE.DoubleSide });
    const ring = new THREE.Mesh(new THREE.RingGeometry(1.075, 1.088, 160), ringMat);
    ring.rotation.x = -Math.PI / 2; ring.position.y = 0.001; scene.add(ring);
    const tickPts = [];
    for (let a = 0; a < 360; a += 2) {
      const r0 = 1.1, r1 = a % 30 === 0 ? 1.17 : a % 10 === 0 ? 1.14 : 1.125, t = (a * Math.PI) / 180;
      tickPts.push(Math.cos(t) * r0, 0.001, Math.sin(t) * r0, Math.cos(t) * r1, 0.001, Math.sin(t) * r1);
    }
    const tickGeo = new THREE.BufferGeometry(); tickGeo.setAttribute("position", new THREE.Float32BufferAttribute(tickPts, 3));
    const tickMat = new THREE.LineBasicMaterial({ color: col("--terrain-line-minor") });
    scene.add(new THREE.LineSegments(tickGeo, tickMat));

    // ---------- Ring labels: top-level folders at their bearing ----------
    const labels = [];
    const centroid = (n) => { const r = rects.get(n); return [(r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2]; };
    const usedAngles = [];
    root.children.slice(0, 8).forEach((n) => {
      const [cx, cy] = centroid(n), [dx, dz] = toDisc(cx, cy);
      const ang = Math.atan2(-dz, dx);
      if (usedAngles.some((a) => Math.abs(Math.atan2(Math.sin(a - ang), Math.cos(a - ang))) < 0.32)) return;
      usedAngles.push(ang);
      const e = document.createElement("div"); e.className = "ring-label"; e.textContent = n.name;
      overlay.appendChild(e);
      labels.push({ el: e, p: new THREE.Vector3(Math.cos(ang) * 1.3, 0, Math.sin(ang) * 1.3) });
    });

    // ---------- Pins ----------
    const leaves = Lustro.leaves(root);
    const pinDefs = [];
    const add = (n, kind) => { if (!pinDefs.some((p) => p.n === n)) pinDefs.push({ n, kind }); };
    if (opts.pins.big) leaves.filter((l) => !l.cloud).sort((a, b) => b.size - a.size).slice(0, 2).forEach((n) => add(n, "big"));
    if (opts.pins.grew) leaves.filter((l) => l.d > 0).sort((a, b) => b.d - a.d).slice(0, 3).forEach((n) => add(n, "grew"));
    if (opts.pins.dup) leaves.filter((l) => l.dup).forEach((n) => add(n, "dup"));
    if (opts.pins.cloud) leaves.filter((l) => l.cloud).forEach((n) => add(n, "cloud"));
    const kindStyle = {
      big: { bg: "var(--text)", icon: Lustro.icons.flag, label: "Largest file" },
      grew: { bg: "var(--grew)", icon: Lustro.icons.arrowUp, label: "Top grower" },
      dup: { bg: "var(--accent)", icon: Lustro.icons.twins, label: "Duplicate" },
      cloud: { bg: "transparent", icon: Lustro.icons.cloud, label: "Cloud only" },
    };
    const pins = pinDefs.map((p, i) => {
      const [cx, cy] = centroid(p.n), [dx, dz] = toDisc(cx, cy), gy = heightAt(cx, cy);
      const s = kindStyle[p.kind];
      const e = document.createElement("div");
      e.className = "pin " + p.kind;
      e.style.color = p.kind === "cloud" ? "var(--cat-cloud)" : s.bg;
      e.innerHTML = `<div class="head" style="background:${s.bg}">${s.icon}</div><div class="stem"></div>`;
      e.onmouseenter = (ev) => Lustro.tip(`<span class="faint">${s.label}</span>` + Lustro.nodeTip(p.n), ev);
      e.onmousemove = (ev) => Lustro.tip(`<span class="faint">${s.label}</span>` + Lustro.nodeTip(p.n), ev);
      e.onmouseleave = () => Lustro.tip(null);
      e.onclick = () => { select(p.n); opts.onPick && opts.onPick(p.n); };
      overlay.appendChild(e);
      return { ...p, el: e, stemEl: e.querySelector(".stem"),
        g: new THREE.Vector3(dx, gy, -dz), t: new THREE.Vector3(dx, gy + 0.32 + (i % 3) * 0.07, -dz) };
    });
    // Start marker for the route (drive root)
    const start = document.createElement("div");
    start.className = "pin start";
    start.style.color = "var(--text)";
    start.innerHTML = `<div class="head" style="background:var(--text);width:24px;height:24px;font:700 10px var(--font-ui);color:var(--surface-1)">${Lustro.data.drive.short}</div><div class="stem"></div>`;
    overlay.appendChild(start);
    const startPin = { el: start, stemEl: start.querySelector(".stem"), g: new THREE.Vector3(), t: new THREE.Vector3() };

    // ---------- Route (dashed path from drive root to selection) ----------
    let routeLine = null;
    const routeMat = new THREE.LineDashedMaterial({ color: col("--text"), dashSize: 0.025, gapSize: 0.02 });
    function buildRoute(node) {
      if (routeLine) { scene.remove(routeLine); routeLine.geometry.dispose(); routeLine = null; }
      if (!node || node === root) { start.style.display = "none"; return; }
      const chain = []; for (let n = node; n && n !== root; n = n.parent) chain.unshift(n);
      const [fx, fy] = centroid(chain[0]);
      const m = Math.max(Math.abs(fx), Math.abs(fy)) || 1;
      const stops = [[fx / m, fy / m]].concat(chain.map(centroid));
      const pts = [];
      for (let k = 0; k < stops.length - 1; k++) {
        const [ax, ay] = stops[k], [bx, by] = stops[k + 1];
        for (let s = 0; s <= 40; s++) {
          const t = s / 40, x = ax + (bx - ax) * t, y = ay + (by - ay) * t, [dx, dz] = toDisc(x, y);
          pts.push(new THREE.Vector3(dx, heightAt(x, y) + 0.012, -dz));
        }
      }
      const g = new THREE.BufferGeometry().setFromPoints(pts);
      routeLine = new THREE.Line(g, routeMat); routeLine.computeLineDistances(); scene.add(routeLine);
      const [sx0, sz0] = toDisc(stops[0][0], stops[0][1]);
      startPin.g.set(sx0, 0, -sz0); startPin.t.set(sx0, 0.16, -sz0); start.style.display = "";
    }

    function rectVec(n) { const r = rects.get(n); return r ? new THREE.Vector4(r.x0, r.y0, r.x1, r.y1) : new THREE.Vector4(9, 9, 9, 9); }
    let selected = null;
    function select(n) {
      selected = n;
      uniforms.uSel.value = n ? rectVec(n) : new THREE.Vector4(9, 9, 9, 9);
      buildRoute(n);
      pins.forEach((p) => p.el.classList.toggle("dim", !!n && p.n !== n && !isAncestor(n, p.n)));
    }
    const isAncestor = (a, b) => { for (let x = b; x; x = x.parent) if (x === a) return true; return false; };

    // ---------- Hover / click on terrain ----------
    const ray = new THREE.Raycaster(), mouse = new THREE.Vector2();
    let hoverLeaf = null, downAt = null;
    function pick(ev) {
      const r = renderer.domElement.getBoundingClientRect();
      mouse.set(((ev.clientX - r.left) / r.width) * 2 - 1, -((ev.clientY - r.top) / r.height) * 2 + 1);
      ray.setFromCamera(mouse, camera);
      const hit = ray.intersectObject(terrain)[0];
      if (!hit) return null;
      const k = hit.face.a;
      return leafAt(sq[k * 2], sq[k * 2 + 1]);
    }
    renderer.domElement.addEventListener("pointermove", (ev) => {
      if (ev.buttons) { Lustro.tip(null); return; }
      const l = pick(ev);
      if (l !== hoverLeaf) { hoverLeaf = l; uniforms.uHover.value = l ? rectVec(l) : new THREE.Vector4(9, 9, 9, 9); }
      if (l) Lustro.tip(Lustro.nodeTip(l), ev); else Lustro.tip(null);
    });
    renderer.domElement.addEventListener("pointerleave", () => { Lustro.tip(null); hoverLeaf = null; uniforms.uHover.value.set(9, 9, 9, 9); });
    renderer.domElement.addEventListener("pointerdown", (ev) => (downAt = [ev.clientX, ev.clientY]));
    renderer.domElement.addEventListener("pointerup", (ev) => {
      if (!downAt || Math.hypot(ev.clientX - downAt[0], ev.clientY - downAt[1]) > 4) return;
      const l = pick(ev);
      if (l) { select(l); opts.onPick && opts.onPick(l); }
    });

    // ---------- Theme ----------
    function retheme() {
      uniforms.uFill.value = col("--terrain-fill"); uniforms.uLine.value = col("--terrain-line");
      uniforms.uMinor.value = col("--terrain-line-minor"); uniforms.uAccent.value = col("--accent");
      uniforms.uCloud.value = col("--cat-cloud");
      uniforms.uLight.value = document.documentElement.dataset.theme === "light" ? 1 : 0;
      baseMat.color = col("--terrain-fill"); ringMat.color = col("--accent"); tickMat.color = col("--terrain-line-minor");
      routeMat.color = col("--text");
      fillTint(); geo.attributes.tint.needsUpdate = true; geo.attributes.cloud.needsUpdate = true;
    }
    document.addEventListener("lustro-theme", retheme);

    // ---------- Render loop ----------
    const v = new THREE.Vector3();
    function project(p) { v.copy(p).project(camera); const r = el.getBoundingClientRect(); return [(v.x * 0.5 + 0.5) * r.width, (-v.y * 0.5 + 0.5) * r.height, v.z]; }
    function placePin(p) {
      const [gx, gy] = project(p.g), [tx, ty, tz] = project(p.t);
      p.el.style.transform = `translate(${tx}px, ${ty}px) translate(-50%, -15px)`;
      p.stemEl.style.height = Math.max(0, gy - ty - 15) + "px";
      p.el.style.zIndex = String(Math.round((1 - tz) * 10000));
    }
    function resize() {
      const r = el.getBoundingClientRect();
      if (!r.width || !r.height) return;
      renderer.setSize(r.width, r.height, false);
      renderer.domElement.style.width = r.width + "px"; renderer.domElement.style.height = r.height + "px";
      camera.aspect = r.width / r.height;
      camera.fov = r.width / r.height < 1.1 ? 40 : 30;
      camera.updateProjectionMatrix();
    }
    new ResizeObserver(resize).observe(el);
    resize();

    // Camera moves ease toward a goal position (and optionally a new look-at point).
    let flatTarget = null, lookGoal = null, alive = true;
    (function loop() {
      if (!alive) return;
      requestAnimationFrame(loop);
      if (lookGoal) {
        controls.target.lerp(lookGoal, 0.12);
        if (controls.target.distanceTo(lookGoal) < 0.005) lookGoal = null;
      }
      if (flatTarget) {
        camera.position.lerp(flatTarget, 0.12);
        if (camera.position.distanceTo(flatTarget) < 0.01) flatTarget = null;
      }
      controls.update();
      renderer.render(scene, camera);
      pins.forEach(placePin);
      if (start.style.display !== "none") placePin(startPin);
      labels.forEach((l) => { const [x, y] = project(l.p); l.el.style.left = x + "px"; l.el.style.top = y + "px"; });
    })();

    // Low-power start (e.g. Linux, whose web engine has weaker 3D): open flat, say why, offer 3D.
    if (opts.lowPower) {
      camera.position.set(0.0001, 4.6, 0.001);
      const note = document.createElement("div");
      note.className = "perf-note";
      note.innerHTML = `<span>Switched to 2D for smoother performance</span><button class="btn small">Try 3D anyway</button>`;
      note.querySelector("button").onclick = () => { note.remove(); flatTarget = new THREE.Vector3(0, 2.55, 3.55); opts.onMode && opts.onMode("3d"); };
      el.appendChild(note);
    }

    return {
      select,
      setTint(on) { uniforms.uTint.value = on ? 1 : 0; },
      setPins(kinds) { pins.forEach((p) => (p.el.style.display = kinds[p.kind] ? "" : "none")); },
      // Fly in on a node: look at it from a raised angle on the side the camera is already on.
      focus(n, dist) {
        const r = rects.get(n); if (!r) return;
        controls.autoRotate = false;
        const cx = (r.x0 + r.x1) / 2, cy = (r.y0 + r.y1) / 2, [dx, dz] = toDisc(cx, cy);
        const at = new THREE.Vector3(dx, Math.max(0, heightAt(cx, cy)) * 0.5, -dz);
        const az = Math.atan2(camera.position.x - controls.target.x, camera.position.z - controls.target.z);
        dist = dist || 1.9;
        lookGoal = at;
        flatTarget = new THREE.Vector3(at.x + Math.sin(az) * dist * 0.75, at.y + dist * 0.7, at.z + Math.cos(az) * dist * 0.75);
      },
      view(mode) {
        controls.autoRotate = false;
        lookGoal = new THREE.Vector3(0, 0.12, 0);
        flatTarget = mode === "top" ? new THREE.Vector3(0.0001, 4.6, 0.001) : mode === "under" ? new THREE.Vector3(0.6, -1.6, 3.4) : new THREE.Vector3(0, 2.55, 3.55);
      },
      leafAt, rects, heightAt,
      destroy() {
        alive = false;
        document.removeEventListener("lustro-theme", retheme);
        controls.dispose(); geo.dispose(); mat.dispose(); floorMat.dispose(); renderer.dispose(); renderer.forceContextLoss();
        el.innerHTML = "";
      },
    };
  };
})();
