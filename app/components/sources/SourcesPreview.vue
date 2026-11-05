<script setup lang="ts">
import type { SourceContent } from '#shared/types'

const props = defineProps<{ path: string }>()

const { data, status, error, refresh } = useFetch<SourceContent>(() => `/api/sources/file/${encodeSourcePath(props.path)}`, {
  key: computed(() => `source-file:${props.path}`),
  // Large binaries are served via ?raw=1; JSON is only needed for text.
  query: { maxChars: 200000 },
})

const kind = computed(() => data.value?.kind)
const rawUrl = computed(() => sourceFileUrl(props.path))
const isText = computed(() => kind.value === 'markdown' || kind.value === 'text')
</script>

<template>
  <div class="space-y-4">
    <div class="flex flex-wrap items-center justify-between gap-2">
      <h2 class="font-mono text-sm text-highlighted break-all">{{ path }}</h2>
      <div class="flex items-center gap-2">
        <UBadge v-if="data" :label="`${data.kind} ・ ${formatBytes(data.size)}`" color="neutral" variant="outline" size="sm" />
        <UButton :to="rawUrl" target="_blank" size="xs" color="neutral" variant="ghost" icon="i-lucide-external-link" label="元ファイル" />
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
        <img :src="rawUrl" :alt="path" class="max-w-full max-h-[75vh] rounded-md border border-default object-contain" loading="lazy">
      </figure>
      <div v-else-if="kind === 'pdf'" class="space-y-3">
        <UButton :to="rawUrl" target="_blank" icon="i-lucide-file-text" label="PDF を開く" />
        <iframe :src="rawUrl" :title="path" class="w-full h-[75vh] rounded-md border border-default" />
      </div>
      <div v-else class="text-sm text-muted">
        プレビューできない形式です。<ULink :to="rawUrl" target="_blank" class="text-primary hover:underline">ダウンロード</ULink>
      </div>
    </template>
  </div>
</template>
