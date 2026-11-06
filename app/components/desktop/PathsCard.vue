<script setup lang="ts">
const { info, getPaths, savePaths, pickFolder } = useDesktop()
const toast = useToast()

const datasetPath = ref('')
const exportPath = ref('')
const settingsFile = ref('')
const datasetError = ref<string | null>(null)
const exportError = ref<string | null>(null)
const saving = ref(false)

async function load() {
  const s = await getPaths()
  datasetPath.value = s.datasetPath
  exportPath.value = s.exportPath
  settingsFile.value = s.settingsFile
  // show problems with the currently stored paths right away
  datasetError.value = s.datasetCheck.error
  exportError.value = s.exportCheck.error
}
await load()

// the banner/retry may change the state; keep the errors in sync with the server state
watch(() => info.value?.phase, () => { load().catch(() => {}) })

async function pick(target: 'dataset' | 'export') {
  const current = target === 'dataset' ? datasetPath.value : exportPath.value || datasetPath.value
  const picked = await pickFolder(current)
  if (!picked) return
  if (target === 'dataset') datasetPath.value = picked
  else exportPath.value = picked
}

async function save() {
  saving.value = true
  datasetError.value = null
  exportError.value = null
  try {
    const out = await savePaths(datasetPath.value, exportPath.value)
    datasetError.value = out.datasetError
    exportError.value = out.exportError
    if (!out.ok) {
      toast.add({ title: 'パスを保存できません', description: '入力内容を確認してください', icon: 'i-lucide-circle-alert', color: 'error' })
      return
    }
    settingsFile.value = out.settings.settingsFile
    if (out.restartError) {
      toast.add({ title: 'パスは保存しましたがサーバーを再起動できません', description: out.restartError, icon: 'i-lucide-circle-alert', color: 'error' })
    } else {
      toast.add({ title: 'パスを保存し、サーバーを再起動しました', icon: 'i-lucide-check', color: 'success' })
    }
  } catch (err) {
    toast.add({ title: '保存に失敗しました', description: String(err), icon: 'i-lucide-circle-alert', color: 'error' })
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <UCard>
    <template #header>
      <h2 class="font-semibold text-highlighted flex items-center gap-2"><UIcon name="i-lucide-folder-cog" class="size-4" />データの場所（デスクトップ版）</h2>
    </template>
    <form class="space-y-4" @submit.prevent="save">
      <UFormField label="データセットパス" name="datasetPath" description="data/<種類>/ のレコードと sources/ を含むフォルダ（読み出し元）" :error="datasetError ?? undefined" required>
        <div class="flex gap-2">
          <UInput v-model="datasetPath" class="w-full font-mono" aria-label="データセットパス" />
          <UButton color="neutral" variant="outline" icon="i-lucide-folder-open" label="選択" @click="pick('dataset')" />
        </div>
      </UFormField>
      <UFormField label="書き出し先パス" name="exportPath" description="保存したレコードの書き込み先（Git 同期の対象）。空欄 = データセットパスと同じ" :error="exportError ?? undefined">
        <div class="flex gap-2">
          <UInput v-model="exportPath" class="w-full font-mono" placeholder="（データセットパスと同じ）" aria-label="書き出し先パス" />
          <UButton color="neutral" variant="outline" icon="i-lucide-folder-open" label="選択" @click="pick('export')" />
        </div>
      </UFormField>
      <p v-if="settingsFile" class="text-xs text-muted break-all">設定ファイル: <span class="font-mono">{{ settingsFile }}</span></p>
      <div class="flex justify-end">
        <UButton type="submit" label="保存して再起動" icon="i-lucide-save" :loading="saving" />
      </div>
    </form>
  </UCard>
</template>
