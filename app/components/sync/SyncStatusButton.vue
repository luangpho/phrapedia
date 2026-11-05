<script setup lang="ts">
defineProps<{ collapsed?: boolean }>()

const open = ref(false)
const { data: status, changeCount, refresh, running } = useSync()

const label = computed(() => {
  if (!status.value) return 'Git 同期'
  if (!status.value.isRepo) return 'Git 未設定'
  if (changeCount.value) return `未コミット ${changeCount.value}`
  if (status.value.ahead) return `未プッシュ ${status.value.ahead}`
  return '同期済み'
})

watch(open, (v) => { if (v) refresh() })
</script>

<template>
  <UModal v-model:open="open" title="Git 同期" description="data/ ディレクトリの変更をコミットし、GitHub と同期します。" :ui="{ content: 'sm:max-w-xl' }">
    <UButton
      color="neutral"
      variant="ghost"
      :icon="running ? 'i-lucide-loader-circle' : 'i-lucide-git-branch'"
      :label="collapsed ? undefined : label"
      :square="collapsed"
      :block="!collapsed"
      :aria-label="`Git 同期: ${label}`"
      class="justify-start"
      :ui="{ leadingIcon: running ? 'animate-spin' : '' }"
    >
      <template v-if="!collapsed && changeCount" #trailing>
        <UBadge :label="changeCount" color="warning" variant="subtle" size="sm" class="ms-auto" />
      </template>
    </UButton>

    <template #body>
      <SyncPanel />
    </template>
  </UModal>
</template>
