<script lang="ts">
  // Terrain overlay: back to the full view. Shown only while the camera isn't at the full view
  // (after a fly-to, a zoom or a pan; orbiting alone doesn't count). The Home key does the same.
  import Icon from "./Icon.svelte";

  interface Props {
    show: boolean;
    /** Small cards: icon only, the label goes in the tooltip. */
    iconOnly?: boolean;
    onclick: () => void;
  }
  let { show, iconOnly = false, onclick }: Props = $props();
</script>

<button class="btn small fullview" class:show class:icon-only={iconOnly} tabindex={show ? 0 : -1} aria-hidden={!show}
  title="Back to the full view (Home)" aria-label={iconOnly ? "Full view" : undefined} {onclick}>
  <Icon name="fullView" size={14} />{#if !iconOnly}Full view{/if}
</button>

<style>
  .fullview {
    position: absolute; left: 12px; bottom: 12px; z-index: 6;
    background: var(--surface-2); border: 1px solid var(--stroke-strong); box-shadow: 0 4px 12px rgba(0,0,0,.3);
    opacity: 0; pointer-events: none; transition: opacity .15s;
  }
  .fullview.show { opacity: 1; pointer-events: auto; }
  .fullview.icon-only { padding: 6px; }
</style>
