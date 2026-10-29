// The 3D view: one heightfield mesh with contour lines drawn in its fragment shader, a base ring
// with compass ticks, and the dashed route. Colors are passed through untouched (sRGB values in,
// sRGB out) so they match the design tokens exactly.

import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import type { Heightmap } from "./heightmap";
import { CONTOUR_STEP, fromDisc, raycastHeightfield, sampleHeight, toDisc } from "./heightmap";
import type { Layout, LayoutNode } from "./layout";
import { centroid } from "./layout";
import type { Palette, RGB } from "./palette";
import { terrainArrays, tintArray } from "./geometry";
import { HOME, UNDER, VIEW_MS, homeOnBearing, isHomeView, viewProgress } from "./rotation";

const HOME_POS = new THREE.Vector3(HOME.pos.x, HOME.pos.y, HOME.pos.z);
const HOME_TARGET = new THREE.Vector3(HOME.target.x, HOME.target.y, HOME.target.z);
const UNDER_POS = new THREE.Vector3(UNDER.pos.x, UNDER.pos.y, UNDER.pos.z);
/** New data eases the heights in over this long, ms (live scans call setData about once a second). */
const HEIGHT_MS = 400;
const NONE = new THREE.Vector4(9, 9, 9, 9);

const raw = (c: RGB) => new THREE.Color().setRGB(c[0], c[1], c[2], THREE.LinearSRGBColorSpace);
const vec = (c: RGB) => new THREE.Vector3(c[0], c[1], c[2]);

// `floor` builds the underside: the same mesh pressed flat to ground level, shown only from below.
const vertexShader = (floor: boolean) => /* glsl */ `
  attribute vec2 sq; attribute vec3 tint; attribute float cloud;
  varying float vH; varying vec2 vSq; varying vec3 vTint; varying float vCloud; varying vec3 vPos;
  void main() {
    vH = position.y; vSq = sq; vTint = tint; vCloud = cloud;
    vec3 p = position; ${floor ? "p.y = -0.002;" : ""}
    vec4 wp = modelMatrix * vec4(p, 1.0); vPos = wp.xyz;
    gl_Position = projectionMatrix * viewMatrix * wp;
  }`;

// Shading matches palette.shade() and the 2D map.
const fragmentShader = (floor: boolean) => /* glsl */ `
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
    // Underside: only from below, and not inside the valleys. Shaded by the height above, so each
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

export class Scene3D {
  readonly canvas: HTMLCanvasElement;
  readonly camera = new THREE.PerspectiveCamera(30, 1, 0.1, 50);
  private renderer: THREE.WebGLRenderer;
  private controls: OrbitControls;
  private scene = new THREE.Scene();
  private geo = new THREE.BufferGeometry();
  private mat: THREE.ShaderMaterial;
  private floorMat: THREE.ShaderMaterial;
  private baseMat = new THREE.MeshBasicMaterial({ side: THREE.DoubleSide });
  private ringMat = new THREE.MeshBasicMaterial({ side: THREE.DoubleSide });
  private tickMat = new THREE.LineBasicMaterial();
  private routeMat = new THREE.LineDashedMaterial({ dashSize: 0.025, gapSize: 0.02 });
  private route: THREE.Line | null = null;
  private hm: Heightmap | null = null;
  /** Current view change (fly-to, full view, look under), time-based. */
  private anim: { p0: THREE.Vector3; p1: THREE.Vector3; t0: THREE.Vector3; t1: THREE.Vector3; start: number } | null = null;
  /** Height ease after new data: from → to, per vertex. */
  private heights: { from: Float32Array; to: Float32Array; start: number } | null = null;
  private dirty = true;
  private ray = new THREE.Raycaster();
  private tmp = new THREE.Vector3();
  /** Duration of view changes, ms (0 = instant, for reduced motion). */
  duration = VIEW_MS;
  /** True while the user is dragging, zooming or touching the view. */
  interacting = false;
  /** Called when the user starts or stops interacting. */
  onInteract: (() => void) | null = null;

  /** Throws if WebGL is unavailable or would run in software (callers fall back to 2D). */
  constructor(container: HTMLElement, pixelRatio: number) {
    this.renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true, failIfMajorPerformanceCaveat: true, powerPreference: "default" });
    this.renderer.outputColorSpace = THREE.LinearSRGBColorSpace;
    this.renderer.setPixelRatio(pixelRatio);
    this.canvas = this.renderer.domElement;
    this.canvas.className = "svt-gl";
    container.appendChild(this.canvas);

    this.camera.position.copy(HOME_POS);
    this.controls = new OrbitControls(this.camera, this.canvas);
    this.controls.target.copy(HOME_TARGET);
    this.controls.enableDamping = true;
    this.controls.minDistance = 1.0;
    this.controls.maxDistance = 6.5;
    this.controls.minPolarAngle = 0.01;
    this.controls.maxPolarAngle = Math.PI - 0.12;
    this.controls.enablePan = false;
    this.controls.autoRotateSpeed = 0.5;
    this.controls.addEventListener("start", this.onStart);
    this.controls.addEventListener("end", this.onEnd);
    this.controls.addEventListener("change", this.onChange);

    this.mat = new THREE.ShaderMaterial({
      uniforms: {
        uFill: { value: new THREE.Vector3() }, uLine: { value: new THREE.Vector3() }, uMinor: { value: new THREE.Vector3() },
        uAccent: { value: new THREE.Vector3() }, uCloud: { value: new THREE.Vector3() },
        uInterval: { value: CONTOUR_STEP }, uTint: { value: 1 }, uLight: { value: 0 },
        uSel: { value: NONE.clone() }, uHover: { value: NONE.clone() },
      },
      vertexShader: vertexShader(false), fragmentShader: fragmentShader(false), side: THREE.DoubleSide,
    });
    // The floor shares the uniforms and the geometry; it only draws when the camera is below ground.
    this.floorMat = new THREE.ShaderMaterial({
      uniforms: this.mat.uniforms, vertexShader: vertexShader(true), fragmentShader: fragmentShader(true), side: THREE.DoubleSide,
    });
    for (const m of [this.mat, this.floorMat]) {
      const mesh = new THREE.Mesh(this.geo, m);
      mesh.frustumCulled = false;
      this.scene.add(mesh);
    }

    const base = new THREE.Mesh(new THREE.RingGeometry(0.995, 1.2, 128), this.baseMat);
    base.rotation.x = -Math.PI / 2; base.position.y = -0.004;
    this.scene.add(base);
    const ring = new THREE.Mesh(new THREE.RingGeometry(1.075, 1.088, 160), this.ringMat);
    ring.rotation.x = -Math.PI / 2; ring.position.y = 0.001;
    this.scene.add(ring);
    const ticks: number[] = [];
    for (let a = 0; a < 360; a += 2) {
      const r0 = 1.1, r1 = a % 30 === 0 ? 1.17 : a % 10 === 0 ? 1.14 : 1.125, t = (a * Math.PI) / 180;
      ticks.push(Math.cos(t) * r0, 0.001, Math.sin(t) * r0, Math.cos(t) * r1, 0.001, Math.sin(t) * r1);
    }
    const tickGeo = new THREE.BufferGeometry();
    tickGeo.setAttribute("position", new THREE.Float32BufferAttribute(ticks, 3));
    this.scene.add(new THREE.LineSegments(tickGeo, this.tickMat));
  }

  private onStart = () => { this.interacting = true; this.anim = null; this.onInteract?.(); };
  private onEnd = () => { this.interacting = false; this.onInteract?.(); };
  private onChange = () => { this.dirty = true; };

  invalidate(): void { this.dirty = true; }

  setTerrain(hm: Heightmap, layout: Layout, palette: Palette): void {
    this.hm = hm;
    const pos = this.geo.getAttribute("position") as THREE.BufferAttribute | undefined;
    if (pos && pos.count === hm.n * hm.n) {
      // Same grid (new data for the same view): update in place and ease the heights, so a live
      // scan's updates grow the land instead of snapping it. The camera is untouched.
      const tint = this.geo.getAttribute("tint") as THREE.BufferAttribute;
      const cloud = this.geo.getAttribute("cloud") as THREE.BufferAttribute;
      (tint.array as Float32Array).set(tintArray(hm, layout, palette)); tint.needsUpdate = true;
      (cloud.array as Float32Array).set(hm.cloud); cloud.needsUpdate = true;
      const arr = pos.array as Float32Array, from = new Float32Array(pos.count);
      for (let k = 0; k < pos.count; k++) from[k] = arr[k * 3 + 1];
      this.heights = { from, to: hm.h.slice(), start: performance.now() };
      if (this.duration <= 0) this.stepHeights(Infinity);
      this.dirty = true;
      return;
    }
    this.heights = null;
    const a = terrainArrays(hm, layout, palette);
    for (const name of ["position", "sq", "tint", "cloud"]) this.geo.deleteAttribute(name);
    this.geo.setAttribute("position", new THREE.BufferAttribute(a.position, 3));
    this.geo.setAttribute("sq", new THREE.BufferAttribute(a.sq, 2));
    this.geo.setAttribute("tint", new THREE.BufferAttribute(a.tint, 3));
    this.geo.setAttribute("cloud", new THREE.BufferAttribute(a.cloud, 1));
    this.geo.setIndex(new THREE.BufferAttribute(a.index, 1));
    this.geo.computeBoundingSphere();
    this.dirty = true;
  }

  setPalette(p: Palette, hm: Heightmap, layout: Layout, tint: boolean): void {
    const u = this.mat.uniforms;
    u.uFill.value = vec(p.fill); u.uLine.value = vec(p.line); u.uMinor.value = vec(p.lineMinor);
    u.uAccent.value = vec(p.accent); u.uCloud.value = vec(p.cloud);
    u.uLight.value = p.light ? 1 : 0; u.uTint.value = tint ? 1 : 0;
    this.baseMat.color = raw(p.fill); this.ringMat.color = raw(p.accent);
    this.tickMat.color = raw(p.lineMinor); this.routeMat.color = raw(p.text);
    const attr = this.geo.getAttribute("tint");
    if (attr) { (attr.array as Float32Array).set(tintArray(hm, layout, p)); attr.needsUpdate = true; }
    this.dirty = true;
  }

  setTint(on: boolean): void { this.mat.uniforms.uTint.value = on ? 1 : 0; this.dirty = true; }

  private rectUniform(name: "uSel" | "uHover", n: LayoutNode | null): void {
    const v = this.mat.uniforms[name].value as THREE.Vector4;
    if (n) v.set(n.x0, n.y0, n.x1, n.y1); else v.copy(NONE);
    this.dirty = true;
  }
  setSelected(n: LayoutNode | null): void { this.rectUniform("uSel", n); }
  setHovered(n: LayoutNode | null): void { this.rectUniform("uHover", n); }

  setRoute(points: Float32Array | null): void {
    if (this.route) { this.scene.remove(this.route); this.route.geometry.dispose(); this.route = null; }
    if (points && points.length >= 6) {
      const g = new THREE.BufferGeometry();
      g.setAttribute("position", new THREE.BufferAttribute(points, 3));
      this.route = new THREE.Line(g, this.routeMat);
      this.route.computeLineDistances();
      this.route.renderOrder = 2;
      this.scene.add(this.route);
    }
    this.dirty = true;
  }

  /** Wheel/pinch zoom. Off in the compact card so scrolling the page passes over it. */
  setZoomEnabled(on: boolean): void { this.controls.enableZoom = on; }

  /** Camera frames the whole map (or the whole Look-under view) and isn't flying anywhere. */
  isHome(under = false): boolean {
    return !this.anim && !this.interacting && isHomeView(this.camera.position, this.controls.target, under);
  }

  private animate(pos: THREE.Vector3, target: THREE.Vector3): void {
    this.anim = { p0: this.camera.position.clone(), p1: pos, t0: this.controls.target.clone(), t1: target, start: performance.now() };
    this.dirty = true;
  }

  private stepHeights(now: number): void {
    const hs = this.heights;
    if (!hs) return;
    const p = viewProgress(hs.start, now, HEIGHT_MS);
    const pos = this.geo.getAttribute("position") as THREE.BufferAttribute, arr = pos.array as Float32Array;
    for (let k = 0; k < hs.to.length; k++) arr[k * 3 + 1] = hs.from[k] + (hs.to[k] - hs.from[k]) * p;
    pos.needsUpdate = true;
    if (p >= 1) { this.heights = null; this.geo.computeBoundingSphere(); }
    this.dirty = true;
  }

  /** Fly in on a patch, looking at it from a raised angle on the side the camera is already on. */
  flyTo(n: LayoutNode, dist = 1.9): void {
    if (!this.hm) return;
    const [cx, cy] = centroid(n), [dx, dz] = toDisc(cx, cy);
    const at = new THREE.Vector3(dx, Math.max(0, sampleHeight(this.hm, cx, cy)) * 0.5, -dz);
    const t = this.controls.target, c = this.camera.position;
    const az = Math.atan2(c.x - t.x, c.z - t.z);
    this.animate(new THREE.Vector3(at.x + Math.sin(az) * dist * 0.75, at.y + dist * 0.7, at.z + Math.cos(az) * dist * 0.75), at);
  }

  /** "home" keeps the current bearing, so returning to the full view never swings round. */
  view(mode: "home" | "under"): void {
    const p = mode === "under" ? UNDER_POS : homeOnBearing(this.camera.position, this.controls.target);
    this.animate(new THREE.Vector3(p.x, p.y, p.z), HOME_TARGET.clone());
  }

  resize(w: number, h: number): void {
    this.renderer.setSize(w, h, false);
    this.camera.aspect = w / h;
    // 30° vertical on wide views; on narrow or tall ones keep the horizontal angle instead,
    // so the whole disc stays in frame.
    const aspect = w / h, wideAspect = 1.6, half = Math.atan(Math.tan((15 * Math.PI) / 180) * wideAspect);
    this.camera.fov = aspect >= wideAspect ? 30 : Math.min(75, (2 * Math.atan(Math.tan(half) / aspect) * 180) / Math.PI);
    this.camera.updateProjectionMatrix();
    this.dirty = true;
  }

  /**
   * Advances camera moves and renders if anything changed. Returns whether it rendered.
   * @param spin auto-rotation speed factor 0..1 (see AutoSpin)
   * @param dt seconds since the last frame, so the rotation speed doesn't depend on frame rate
   */
  update(spin = 0, dt = 1 / 60): boolean {
    const now = performance.now();
    if (this.anim) {
      const a = this.anim, p = viewProgress(a.start, now, this.duration);
      this.camera.position.lerpVectors(a.p0, a.p1, p);
      this.controls.target.lerpVectors(a.t0, a.t1, p);
      if (p >= 1) this.anim = null;
      this.dirty = true;
    }
    if (this.heights) this.stepHeights(now);
    this.controls.autoRotate = spin > 0;
    this.controls.autoRotateSpeed = 0.5 * spin;
    this.controls.update(Math.min(dt, 0.1)); // fires "change" (→ dirty) while damping or auto-rotating
    if (!this.dirty) return false;
    this.dirty = false;
    this.renderer.render(this.scene, this.camera);
    return true;
  }

  /** Square coordinates under a client-space point, or null if it misses the terrain. */
  pick(clientX: number, clientY: number): [number, number] | null {
    if (!this.hm) return null;
    const r = this.canvas.getBoundingClientRect();
    if (!r.width || !r.height) return null;
    const ndc = new THREE.Vector2(((clientX - r.left) / r.width) * 2 - 1, -((clientY - r.top) / r.height) * 2 + 1);
    this.ray.setFromCamera(ndc, this.camera);
    const o = this.ray.ray.origin, d = this.ray.ray.direction;
    // From below, the solid floor at ground level hides the land; only valleys dip beneath it.
    if (o.y < 0 && d.y > 0) {
      const t = -o.y / d.y, x = o.x + d.x * t, z = o.z + d.z * t;
      if (x * x + z * z <= 1) {
        const [sx, sy] = fromDisc(x, -z);
        if (sampleHeight(this.hm, sx, sy) >= 0) return [sx, sy];
      }
    }
    const hit = raycastHeightfield(this.hm, [o.x, o.y, o.z], [d.x, d.y, d.z]);
    return hit ? [hit.sx, hit.sy] : null;
  }

  /** World point → element pixels; z > 1 means behind the camera. */
  project(x: number, y: number, z: number, w: number, h: number): [number, number, number] {
    const v = this.tmp.set(x, y, z).project(this.camera);
    return [(v.x * 0.5 + 0.5) * w, (-v.y * 0.5 + 0.5) * h, v.z];
  }

  dispose(): void {
    this.controls.removeEventListener("start", this.onStart);
    this.controls.removeEventListener("end", this.onEnd);
    this.controls.removeEventListener("change", this.onChange);
    this.controls.dispose();
    this.setRoute(null);
    this.scene.traverse((o) => {
      const m = o as THREE.Mesh;
      if (m.geometry) m.geometry.dispose();
    });
    for (const m of [this.mat, this.floorMat, this.baseMat, this.ringMat, this.tickMat, this.routeMat]) m.dispose();
    this.renderer.dispose();
    this.renderer.forceContextLoss();
    this.canvas.remove();
  }
}
