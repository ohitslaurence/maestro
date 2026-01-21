<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
  import Badge from './Badge.svelte';

  interface Props {
    variant?: 'default' | 'accent' | 'success' | 'warning' | 'error' | 'info' | 'muted';
    size?: 'sm' | 'md';
    text?: string;
  }

  let { variant = 'default', size = 'md', text = 'Badge' }: Props = $props();
</script>

<Badge {variant} {size}>
  {text}
</Badge>
