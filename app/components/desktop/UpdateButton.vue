<script setup lang="ts">
/** Header update icon (desktop only, and only when this build has the updater configured). */
const { state, enabled, currentVersion, handleClick } = useUpdater()
const toast = useToast()

const busy = computed(() => ['checking', 'downloading', 'installing'].includes(state.value.kind))

const tooltip = computed(() => {
  const s = state.value
  const cur = currentVersion.value ? `現在 ${currentVersion.value}` : ''
  switch (s.kind) {
    case 'available': return `新しい版 ${s.version} があります（クリックで更新）${cur ? `\n${cur}` : ''}`
    case 'checking': return '更新を確認中…'
    case 'downloading': return `ダウンロード中 ${s.version}${s.percent === null ? '' : ` ${s.percent}%`}`
    case 'installing': return '更新をインストール中…（まもなく再起動します）'
    case 'error': return `更新に失敗しました: ${s.message}\nクリックで再確認${cur ? `\n${cur}` : ''}`
    default: return `更新を確認${cur ? `（${cur}）` : ''}`
  }
})

const icon = computed(() => {
  switch (state.value.kind) {
    case 'checking': return 'i-lucide-loader-circle'
    case 'downloading':
    case 'installing': return 'i-lucide-loader-circle'
    case 'error': return 'i-lucide-circle-alert'
    case 'available': return 'i-lucide-download'
    default: return 'i-lucide-refresh-cw'
  }
})

const percentLabel = computed(() => (state.value.kind === 'downloading' && state.value.percent !== null ? `${state.value.percent}%` : ''))

function onClick() {
  void handleClick(t => toast.add({ title: t.title, description: t.description, color: t.color, icon: t.color === 'success' ? 'i-lucide-check' : 'i-lucide-circle-alert', duration: t.color === 'error' ? 15000 : undefined }))
}
</script>

<template>
  <div v-if="enabled" class="flex items-center gap-1">
    <span v-if="percentLabel" class="text-xs text-muted tabular-nums" role="status">{{ percentLabel }}</span>
    <UTooltip :text="tooltip" :ui="{ text: 'whitespace-pre-line' }">
      <UChip :show="state.kind === 'available'" color="primary" size="md" inset>
        <UButton
          :icon="icon"
          :color="state.kind === 'error' ? 'error' : state.kind === 'available' ? 'primary' : 'neutral'"
          variant="ghost"
          square
          size="sm"
          :disabled="busy"
          :loading="false"
          :ui="{ leadingIcon: busy ? 'animate-spin' : '' }"
          :aria-label="state.kind === 'available' ? `更新（新しい版 ${state.version}）` : '更新を確認'"
          data-testid="update-button"
          :data-updater-state="state.kind"
          @click="onClick"
        />
      </UChip>
    </UTooltip>
  </div>
</template>
