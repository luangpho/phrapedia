<script setup lang="ts">
const props = withDefaults(defineProps<{
  source: string
  /** Remove a leading YAML frontmatter block before rendering (default true). */
  stripFrontmatter?: boolean
  emptyText?: string
}>(), { stripFrontmatter: true, emptyText: '本文はまだありません' })

const { render } = useMarkdown()

// Rendering (DOMPurify) needs the DOM, so it only happens on the client; the server
// renders a lightweight placeholder that is swapped after hydration (no mismatch).
const mounted = ref(false)
onMounted(() => { mounted.value = true })

const html = computed(() => (mounted.value ? render(props.source ?? '', { stripFrontmatter: props.stripFrontmatter }) : ''))
const root = ref<HTMLElement | null>(null)

// Primary-source images/links are marker hrefs (see rewriteSourceHref): load the bytes through the API and swap in Blob URLs.
async function resolveSources() {
  await nextTick()
  for (const img of Array.from(root.value?.querySelectorAll<HTMLImageElement>('img') ?? [])) {
    const path = sourcePathFromHref(img.getAttribute('src'))
    if (!path) continue
    img.alt = img.alt || path
    try {
      img.src = await sourceBlobUrl(path)
    } catch (err) {
      img.title = errorMessage(err)
    }
  }
}
watch(html, () => { void resolveSources() }, { flush: 'post' })

async function onClick(e: MouseEvent) {
  const a = (e.target as HTMLElement | null)?.closest('a')
  const path = sourcePathFromHref(a?.getAttribute('href'))
  if (!path) return
  e.preventDefault()
  try {
    window.open(await sourceBlobUrl(path), '_blank')
  } catch (err) {
    useToast().add({ title: 'ファイルを開けませんでした', description: errorMessage(err), color: 'error', icon: 'i-lucide-circle-alert' })
  }
}

const isEmpty = computed(() => !(props.stripFrontmatter ? stripFrontmatter(props.source ?? '') : props.source ?? '').trim())
</script>

<template>
  <p v-if="isEmpty" class="text-sm text-muted italic">
    {{ emptyText }}
  </p>
  <!-- eslint-disable-next-line vue/no-v-html -- sanitized with DOMPurify -->
  <div v-else-if="mounted" ref="root" class="phra-prose" @click="onClick" v-html="html" />
  <div v-else class="space-y-3" aria-busy="true">
    <USkeleton class="h-7 w-1/2" />
    <USkeleton class="h-4 w-full" />
    <USkeleton class="h-4 w-5/6" />
    <USkeleton class="h-4 w-2/3" />
  </div>
</template>
