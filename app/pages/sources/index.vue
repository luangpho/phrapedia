<script setup lang="ts">
import type { SourceFile, SourceFileKind, SourceSearchHit } from '#shared/types'

useHead({ title: '一次資料 - Phrapedia' })

const route = useRoute()
const router = useRouter()

const { data: files, status, error, refresh } = useAsyncData<SourceFile[]>('sources', () => api.listSources(), { default: () => [] })

const selected = computed(() => (typeof route.query.path === 'string' ? route.query.path : ''))
function select(path: string) {
  router.replace({ query: { ...route.query, path } })
}

// ---- grouped list ----
const filter = ref('')
const groups = computed(() => {
  const f = filter.value.trim().toLowerCase()
  const map = new Map<string, SourceFile[]>()
  for (const file of files.value ?? []) {
    if (f && !file.path.toLowerCase().includes(f)) continue
    const i = file.path.lastIndexOf('/')
    const dir = i >= 0 ? file.path.slice(0, i) : ''
    if (!map.has(dir)) map.set(dir, [])
    map.get(dir)!.push(file)
  }
  return [...map.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([dir, list]) => ({ dir, files: list.sort((a, b) => a.name.localeCompare(b.name)) }))
})

const KIND_ICON: Record<SourceFileKind, string> = {
  markdown: 'i-lucide-file-text',
  text: 'i-lucide-file',
  image: 'i-lucide-image',
  pdf: 'i-lucide-file-type',
  other: 'i-lucide-file-question',
}

// ---- full-text search ----
const q = ref(typeof route.query.q === 'string' ? route.query.q : '')
const hits = ref<SourceSearchHit[] | null>(null)
const searching = ref(false)
const searchError = ref<string | null>(null)
let timer: ReturnType<typeof setTimeout> | undefined
let seq = 0

async function runSearch() {
  const query = q.value.trim()
  if (!query) {
    hits.value = null
    searchError.value = null
    return
  }
  const my = ++seq
  searching.value = true
  searchError.value = null
  try {
    const res = await api.searchSources(query, 30)
    if (my === seq) hits.value = res
  } catch (err) {
    if (my === seq) searchError.value = errorMessage(err)
  } finally {
    if (my === seq) searching.value = false
  }
}

watch(q, () => {
  clearTimeout(timer)
  timer = setTimeout(runSearch, 300)
})
onMounted(() => { if (q.value) runSearch() })
onBeforeUnmount(() => clearTimeout(timer))
</script>

<template>
  <UDashboardPanel id="sources" :ui="{ body: 'p-0 sm:p-0 gap-0 sm:gap-0 overflow-hidden' }">
    <template #header>
      <UDashboardNavbar title="一次資料">
        <template #right>
          <UButton color="neutral" variant="ghost" icon="i-lucide-rotate-ccw" aria-label="一覧を再読み込み" :loading="status === 'pending'" @click="refresh()" />
        </template>
      </UDashboardNavbar>
    </template>

    <template #body>
      <div class="flex flex-1 min-h-0 flex-col md:flex-row">
        <!-- list / search -->
        <div
          class="md:w-80 lg:w-96 shrink-0 md:border-e border-default flex flex-col min-h-0"
          :class="selected ? 'hidden md:flex' : 'flex flex-1 md:flex-none'"
        >
          <div class="p-3 space-y-2 border-b border-default">
            <UInput v-model="q" icon="i-lucide-search" placeholder="全文検索（本文）" class="w-full" aria-label="一次資料を全文検索" :loading="searching">
              <template v-if="q" #trailing>
                <UButton color="neutral" variant="link" size="xs" icon="i-lucide-x" aria-label="検索をクリア" @click="q = ''" />
              </template>
            </UInput>
            <UInput v-if="!q" v-model="filter" icon="i-lucide-filter" placeholder="ファイル名で絞り込み" size="sm" class="w-full" aria-label="ファイル名で絞り込み" />
          </div>

          <div class="flex-1 min-h-0 overflow-y-auto p-2">
            <!-- search results -->
            <template v-if="q.trim()">
              <p v-if="searchError" class="p-2 text-sm text-error">{{ searchError }}</p>
              <div v-else-if="searching && !hits" class="space-y-2 p-2">
                <USkeleton v-for="i in 4" :key="i" class="h-12 w-full" />
              </div>
              <p v-else-if="hits && !hits.length" class="p-2 text-sm text-muted">「{{ q }}」に一致する資料はありません</p>
              <ul v-else-if="hits" class="space-y-1">
                <li v-for="hit in hits" :key="hit.path">
                  <button
                    type="button"
                    class="w-full text-start rounded-md px-2 py-2 hover:bg-elevated/60 focus-visible:outline-2 focus-visible:outline-primary"
                    :class="selected === hit.path ? 'bg-elevated' : ''"
                    :aria-current="selected === hit.path ? 'true' : undefined"
                    @click="select(hit.path)"
                  >
                    <span class="flex items-center gap-1.5 text-sm font-medium text-highlighted">
                      <UIcon :name="KIND_ICON[hit.kind]" class="size-4 shrink-0 text-muted" />
                      <span class="truncate font-mono text-xs">{{ hit.path }}</span>
                    </span>
                    <span v-if="hit.snippet" class="mt-1 block text-xs text-muted line-clamp-3">{{ hit.snippet }}</span>
                  </button>
                </li>
              </ul>
            </template>

            <!-- grouped file list -->
            <template v-else>
              <div v-if="status === 'pending' && !files?.length" class="space-y-2 p-2" aria-busy="true">
                <USkeleton v-for="i in 6" :key="i" class="h-6 w-full" />
              </div>
              <div v-else-if="error" class="p-2 space-y-2 text-sm">
                <p class="text-error">一覧を取得できませんでした</p>
                <p class="text-xs text-muted">{{ errorMessage(error) }}</p>
                <UButton size="xs" color="neutral" variant="outline" icon="i-lucide-rotate-ccw" label="再読み込み" @click="refresh()" />
              </div>
              <div v-else-if="!files?.length" class="p-3 text-sm text-muted space-y-1">
                <p>一次資料がまだありません。</p>
                <p>設定の「一次資料フォルダ」に Markdown や画像を置いてください。</p>
                <ULink to="/settings" class="text-primary hover:underline">設定を開く</ULink>
              </div>
              <p v-else-if="!groups.length" class="p-2 text-sm text-muted">一致するファイルはありません</p>
              <div v-else class="space-y-3">
                <section v-for="g in groups" :key="g.dir">
                  <h3 class="px-2 py-1 text-xs font-medium text-muted flex items-center gap-1.5">
                    <UIcon name="i-lucide-folder" class="size-3.5" />{{ g.dir || '（ルート）' }}
                  </h3>
                  <ul>
                    <li v-for="file in g.files" :key="file.path">
                      <button
                        type="button"
                        class="w-full flex items-center gap-2 rounded-md px-2 py-1.5 text-sm text-start hover:bg-elevated/60 focus-visible:outline-2 focus-visible:outline-primary"
                        :class="selected === file.path ? 'bg-elevated text-highlighted' : 'text-default'"
                        :aria-current="selected === file.path ? 'true' : undefined"
                        @click="select(file.path)"
                      >
                        <UIcon :name="KIND_ICON[file.kind]" class="size-4 shrink-0 text-muted" />
                        <span class="truncate flex-1">{{ file.name }}</span>
                        <span class="text-xs text-dimmed tabular-nums">{{ formatBytes(file.size) }}</span>
                      </button>
                    </li>
                  </ul>
                </section>
              </div>
            </template>
          </div>
        </div>

        <!-- preview -->
        <div class="flex-1 min-w-0 min-h-0 overflow-y-auto p-4 sm:p-6" :class="selected ? '' : 'hidden md:block'">
          <UButton v-if="selected" class="md:hidden mb-3" size="sm" color="neutral" variant="ghost" icon="i-lucide-arrow-left" label="一覧へ" @click="router.replace({ query: {} })" />
          <SourcesPreview v-if="selected" :key="selected" :path="selected" />
          <div v-else class="h-full flex flex-col items-center justify-center text-center text-muted gap-2">
            <UIcon name="i-lucide-library" class="size-10 text-dimmed" />
            <p class="text-sm">左の一覧からファイルを選ぶと内容を表示します</p>
          </div>
        </div>
      </div>
    </template>
  </UDashboardPanel>
</template>
