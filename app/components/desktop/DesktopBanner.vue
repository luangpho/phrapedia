<script setup lang="ts">
const { info, isDesktop, reload } = useDesktop()
const retrying = ref(false)

const messages = computed(() => {
  const i = info.value
  if (!i) return []
  const out: string[] = []
  if (i.error) out.push(`${i.error}`)
  if (i.settingsWarning) out.push(i.settingsWarning)
  if (!i.datasetCheck.ok && i.datasetCheck.error) out.push(`データセットパス: ${i.datasetCheck.error}`)
  if (!i.exportCheck.ok && i.exportCheck.error) out.push(`書き出し先パス: ${i.exportCheck.error}`)
  return out
})

async function retry() {
  retrying.value = true
  try {
    await reload()
    await refreshNuxtData()
  } catch (err) {
    if (info.value) info.value = { ...info.value, error: `アプリの状態を取得できません: ${errorMessage(err)}` }
  } finally {
    retrying.value = false
  }
}
</script>

<template>
  <div v-if="isDesktop && messages.length" class="fixed inset-x-0 top-0 z-50 p-2" role="alert">
    <UAlert
      color="error"
      variant="solid"
      icon="i-lucide-circle-alert"
      title="デスクトップ設定に問題があります"
      :actions="[
        { label: '設定を開く', to: '/settings', color: 'neutral', variant: 'outline' },
        { label: '再試行', color: 'neutral', variant: 'outline', loading: retrying, onClick: retry },
      ]"
    >
      <template #description>
        <ul class="list-disc pl-4 break-all">
          <li v-for="m in messages" :key="m" class="whitespace-pre-line">{{ m }}</li>
        </ul>
      </template>
    </UAlert>
  </div>
</template>
