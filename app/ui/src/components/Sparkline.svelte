<script lang="ts">
  let { values, color }: { values: number[]; color: string } = $props();
  const w = 80, h = 22;
  const pts = $derived.by(() => {
    if (values.length < 2) return "";
    const lo = Math.min(...values), hi = Math.max(...values);
    return values.map((v, k) => `${(k / (values.length - 1)) * w},${h - 2 - ((v - lo) / (hi - lo || 1)) * (h - 4)}`).join(" ");
  });
</script>

<svg class="spark" viewBox="0 0 {w} {h}" aria-hidden="true">
  <polyline points={pts} fill="none" stroke="var({color})" stroke-width="1.6" stroke-linejoin="round" />
</svg>

<style>
  .spark { width: 80px; height: 22px; display: block; margin-left: auto; }
</style>
