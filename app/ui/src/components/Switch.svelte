<script lang="ts">
  let { on, locked = false, label, onchange }: { on: boolean; locked?: boolean; label: string; onchange?: (on: boolean) => void } = $props();
</script>

<button class="switch" class:on role="switch" aria-checked={on} aria-label={label} disabled={locked}
  title={locked ? "Always on" : undefined} onclick={() => onchange?.(!on)}></button>
