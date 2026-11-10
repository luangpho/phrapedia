<script setup lang="ts">
import type { SourceContent } from '#shared/types'

const props = defineProps<{ path: string }>()

const { data, status, error, refresh } = useAsyncData<SourceContent>(
  () => `source-file:${props.path}`,
  // Large binaries are loaded as bytes below; the content call is only needed for text.
  () => api.readSource(props.path, 200000),
  { watch: [() => props.path] },
)

const kind = computed(() => data.value?.kind)
const isText = computed(() => kind.value === 'markdown' || kind.value === 'text')

// images / PDFs / other binaries: Blob URL from read_source_bytes
const rawUrl = ref<string | null>(null)
const rawError = ref<string | null>(null)
watch([() => props.path, kind], async ([path, k]) => {
  rawUrl.value = null
  rawError.value = null
  if (!k || k === 'markdown' || k === 'text') return
  try {
    const url = await sourceBlobUrl(path)
    if (path === props.path) rawUrl.value = url
  } catch (err) {
    if (path === props.path) rawError.value = errorMessage(err)
  }
}, { immediate: true })
</script>

<template>
  <div class="space-y-4">
    <div class="flex flex-wrap items-center justify-between gap-2">
      <h2 class="font-mono text-sm text-highlighted break-all">{{ path }}</h2>
      <div class="flex items-center gap-2">
        <UBadge v-if="data" :label="`${data.kind} ・ ${formatBytes(data.size)}`" color="neutral" variant="outline" size="sm" />
        <UButton v-if="rawUrl" :to="rawUrl" target="_blank" size="xs" color="neutral" variant="ghost" icon="i-lucide-external-link" label="元ファイル" />
      </div>
    </div>

    <div v-if="status === 'pending' && !data" class="space-y-2" aria-busy="true">
      <USkeleton class="h-5 w-1/3" />
      <USkeleton class="h-4 w-full" />
      <USkeleton class="h-4 w-4/5" />
    </div>

    <UAlert
      v-else-if="error"
      color="error"
      variant="subtle"
      icon="i-lucide-circle-alert"
      title="ファイルを読み込めませんでした"
      :description="errorMessage(error)"
      :actions="[{ label: '再試行', color: 'neutral', variant: 'outline', onClick: () => { refresh() } }]"
    />

    <template v-else-if="data">
      <MarkdownView v-if="kind === 'markdown'" :source="data.text ?? ''" empty-text="（空のファイル）" />
      <pre v-else-if="isText" class="whitespace-pre-wrap break-words text-sm font-mono rounded-md border border-default bg-elevated/40 p-4">{{ data.text }}</pre>
      <figure v-else-if="kind === 'image'" class="space-y-2">
        <UAlert v-if="rawError" color="error" variant="subtle" icon="i-lucide-circle-alert" title="画像を読み込めませんでした" :description="rawError" />
        <img v-else-if="rawUrl" :src="rawUrl" :alt="path" class="max-w-full max-h-[75vh] rounded-md border border-default object-contain" loading="lazy">
      </figure>
      <div v-else-if="kind === 'pdf'" class="space-y-3">
        <UAlert v-if="rawError" color="error" variant="subtle" icon="i-lucide-circle-alert" title="PDF を読み込めませんでした" :description="rawError" />
        <UButton v-if="rawUrl" :to="rawUrl" target="_blank" icon="i-lucide-file-text" label="PDF を開く" />
        <iframe v-if="rawUrl" :src="rawUrl" :title="path" class="w-full h-[75vh] rounded-md border border-default" />
      </div>
      <div v-else class="text-sm text-muted">
        プレビューできない形式です。<ULink v-if="rawUrl" :to="rawUrl" target="_blank" class="text-primary hover:underline">ダウンロード</ULink>
      </div>
    </template>
  </div>
</template>
