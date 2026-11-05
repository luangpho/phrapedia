<script setup lang="ts">
const props = withDefaults(defineProps<{ status?: string; size?: 'xs' | 'sm' | 'md' }>(), { size: 'sm' })
const meta = computed(() => statusMeta(props.status))
</script>

<template>
  <UBadge :label="meta.label" :color="meta.color" variant="subtle" :size="size" class="shrink-0" />
</template>
