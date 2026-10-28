<script lang="ts">
  import type { CloudInfo } from "../ipc/types";
  import { CLOUD_STATE, PROVIDER } from "../lib/labels";
  import { icons } from "../lib/icons";
  let { cloud, mini = false }: { cloud: CloudInfo; mini?: boolean } = $props();
  const ic = $derived(cloud.state === "cloudOnly" ? icons.cloud : cloud.state === "downloaded" ? icons.cloudDown : icons.pin);
</script>

<span class="cmark {cloud.state === 'pinned' ? 'pinned' : ''}" class:mini title="{PROVIDER[cloud.provider].label} · {CLOUD_STATE[cloud.state].label}"
  >{PROVIDER[cloud.provider].short}{@html ic}</span>
