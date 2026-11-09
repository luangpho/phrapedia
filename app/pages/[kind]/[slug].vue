<script setup lang="ts">
import type { EntityKind, EntityRecord } from '#shared/types'

definePageMeta({
  validate: route => ['monks', 'temples', 'amulets'].includes(String(route.params.kind)),
})

const route = useRoute()
const kind = computed(() => String(route.params.kind) as EntityKind)
const slug = computed(() => String(route.params.slug))

const toast = useToast()
const { update, remove } = useEntities()

const { data: record, status, error, refresh } = await useFetch<EntityRecord>(
  () => `/api/entities/${kind.value}/${encodeURIComponent(slug.value)}`,
  { key: computed(() => entityKey(kind.value, slug.value)) },
)

useHead({ title: () => `${record.value?.name ?? slug.value} - Phrapedia` })

// ---- editor state ----
const editing = ref(false)
const editorRaw = ref(record.value?.raw ?? '')
const saving = ref(false)
const dirty = computed(() => !!record.value && editorRaw.value !== record.value.raw)

watch(() => record.value?.raw, (raw) => {
  // Follow server data while there are no local edits.
  if (raw != null && !editing.value) editorRaw.value = raw
})

useUnsavedGuard(dirty)

function startEdit() {
  if (record.value && !dirty.value) editorRaw.value = record.value.raw
  editing.value = true
}

async function cancelEdit() {
  if (dirty.value && !(await confirmDialog('編集内容を破棄しますか？'))) return
  editorRaw.value = record.value?.raw ?? ''
  editing.value = false
  draftNotice.value = null
}

async function save() {
  if (!record.value || saving.value) return
  if (!dirty.value) {
    toast.add({ title: '変更はありません', icon: 'i-lucide-info', color: 'neutral' })
    return
  }
  saving.value = true
  try {
    const res = await update(kind.value, slug.value, editorRaw.value)
    record.value = res.record
    editorRaw.value = res.record.raw
    editing.value = false
    draftNotice.value = null
    toast.add({ title: '保存しました', description: res.record.path, icon: 'i-lucide-check', color: 'success' })
    if (res.record.slug !== slug.value) await navigateTo(`/${kind.value}/${res.record.slug}`)
  } catch (err) {
    toast.add({ title: '保存に失敗しました', description: errorMessage(err), icon: 'i-lucide-circle-alert', color: 'error' })
  } finally {
    saving.value = false
  }
}

// ---- AI chat ----
const chatOpen = ref(false)
const draftNotice = ref<string | null>(null)

function onDraft(markdown: string, summary?: string) {
  editorRaw.value = markdown
  editing.value = true
  draftNotice.value = summary || ''
}

// ---- delete ----
const deleteOpen = ref(false)
const deleting = ref(false)
/** true once the record was deleted; the page shows a neutral placeholder until navigation completes */
const deleted = ref(false)

async function confirmDelete() {
  if (!record.value) return
  deleting.value = true
  try {
    const path = record.value.path
    deleted.value = true
    await remove(kind.value, slug.value)
    editing.value = false
    deleteOpen.value = false
    toast.add({ title: '削除しました', description: path, icon: 'i-lucide-trash-2', color: 'success' })
    await navigateTo('/')
  } catch (err) {
    toast.add({ title: '削除に失敗しました', description: errorMessage(err), icon: 'i-lucide-circle-alert', color: 'error' })
  } finally {
    deleting.value = false
  }
}

const subtitle = computed(() => [record.value?.name_th, record.value?.name_en].filter(Boolean).join(' ・ '))
const notFound = computed(() => (error.value as { statusCode?: number } | null)?.statusCode === 404)
// SSR: report the real status for a missing/failed record instead of 200.
if (import.meta.server && error.value) {
  setResponseStatus(useRequestEvent()!, notFound.value ? 404 : 500)
}

/** The AI saved this (or another) record through save_entity: reload it and drop local edits that are now stale. */
async function onSaved(saved: { kind: EntityKind; slug: string; path?: string }) {
  await useEntities().refreshList(saved.kind)
  if (saved.kind === kind.value && saved.slug === slug.value) {
    await refresh()
    editorRaw.value = record.value?.raw ?? editorRaw.value
    editing.value = false
    draftNotice.value = null
    toast.add({ title: 'AI がレコードを保存しました', description: saved.path, icon: 'i-lucide-check', color: 'success' })
  } else {
    toast.add({ title: 'AI がレコードを保存しました', description: saved.path, icon: 'i-lucide-check', color: 'success', actions: [{ label: '開く', to: `/${saved.kind}/${saved.slug}` }] })
  }
}
</script>

<template>
  <UDashboardPanel :id="`record-${kind}`" :ui="{ body: 'p-0 sm:p-0 gap-0 sm:gap-0 overflow-hidden' }">
    <template #header>
      <UDashboardNavbar :ui="{ right: 'gap-1.5', title: 'min-w-0' }">
        <template #title>
          <div v-if="record" class="flex items-center gap-2 min-w-0">
            <div class="min-w-0">
              <span class="block truncate">{{ record.name }}</span>
              <span v-if="subtitle" class="block truncate text-xs font-normal text-muted">{{ subtitle }}</span>
            </div>
            <EntityStatusBadge :status="record.status" />
            <UBadge v-if="dirty" label="未保存" color="warning" variant="soft" size="sm" icon="i-lucide-circle-dot" />
          </div>
          <USkeleton v-else-if="status === 'pending'" class="h-5 w-40" />
          <span v-else>{{ kindLabel(kind) }}</span>
        </template>

        <template v-if="record" #right>
          <template v-if="editing">
            <UButton color="neutral" variant="ghost" icon="i-lucide-x" aria-label="取消" :disabled="saving" @click="cancelEdit">
              <span class="hidden md:inline">取消</span>
            </UButton>
            <UButton icon="i-lucide-save" aria-label="保存" :loading="saving" :disabled="!dirty" @click="save">
              <span class="hidden md:inline">保存</span>
            </UButton>
          </template>
          <UButton v-else color="neutral" variant="outline" icon="i-lucide-pencil" aria-label="編集" @click="startEdit">
            <span class="hidden md:inline">編集</span>
          </UButton>
          <UButton
            color="neutral"
            :variant="chatOpen ? 'soft' : 'ghost'"
            icon="i-lucide-sparkles"
            aria-label="AIに相談"
            :aria-pressed="chatOpen"
            @click="chatOpen = !chatOpen"
          >
            <span class="hidden md:inline">AIに相談</span>
          </UButton>
          <UButton color="error" variant="ghost" icon="i-lucide-trash-2" aria-label="削除" @click="deleteOpen = true" />
        </template>
      </UDashboardNavbar>
    </template>

    <template #body>
      <div v-if="status === 'pending' && !record" class="p-4 sm:p-6 space-y-4 max-w-3xl" aria-busy="true">
        <USkeleton class="h-8 w-1/3" />
        <USkeleton class="h-4 w-full" />
        <USkeleton class="h-4 w-5/6" />
        <USkeleton class="h-4 w-2/3" />
      </div>

      <div v-else-if="deleted" class="p-4 sm:p-6 text-sm text-muted">削除しました。</div>
      <div v-else-if="error || !record" class="p-4 sm:p-6 max-w-xl">
        <UAlert
          :color="notFound ? 'neutral' : 'error'"
          variant="subtle"
          :icon="notFound ? 'i-lucide-file-question' : 'i-lucide-circle-alert'"
          :title="notFound ? 'レコードが見つかりません' : '読み込みに失敗しました'"
          :description="notFound ? `${kind}/${slug} は存在しないか削除されました。` : errorMessage(error)"
          :actions="notFound
            ? [{ label: 'ホームへ', to: '/', color: 'neutral', variant: 'outline' }, { label: `${kindLabel(kind)}を追加`, to: `/${kind}/new`, icon: 'i-lucide-plus' }]
            : [{ label: '再試行', color: 'neutral', variant: 'outline', onClick: () => { refresh() } }]"
        />
      </div>

      <EntityWorkspace
        v-else
        v-model:open="chatOpen"
        :kind="kind"
        :slug="slug"
        :draft="editorRaw"
        @draft="onDraft"
        @saved="onSaved"
      >
        <UAlert
          v-if="draftNotice !== null"
          class="mb-4"
          color="primary"
          variant="subtle"
          icon="i-lucide-sparkles"
          title="AIが下書きを更新しました"
          :description="draftNotice || '内容を確認して保存してください。'"
          close
          @update:open="(v: boolean) => { if (!v) draftNotice = null }"
        />

        <MarkdownEditor v-if="editing" v-model="editorRaw" :disabled="saving" @save="save" />

        <div
          v-else
          class="grid gap-6"
          :class="chatOpen ? '' : '2xl:grid-cols-[minmax(0,1fr)_18rem]'"
        >
          <article class="min-w-0 max-w-3xl">
            <MarkdownView :source="record.body" :strip-frontmatter="false" />
          </article>
          <aside class="order-first" :class="chatOpen ? '' : '2xl:order-last'" aria-label="基本情報">
            <EntityMetaCard :kind="kind" :frontmatter="record.frontmatter" :path="record.path" />
          </aside>
        </div>
      </EntityWorkspace>

      <UModal v-model:open="deleteOpen" title="レコードを削除しますか？" :ui="{ footer: 'justify-end' }">
        <template #body>
          <div class="space-y-2 text-sm">
            <p>「{{ record?.name }}」を削除します。次のファイルが削除されます。</p>
            <p class="font-mono text-xs rounded-md bg-elevated px-2 py-1.5 break-all">{{ record?.path }}</p>
            <p class="text-muted">Git にコミット済みであれば履歴から復元できます。</p>
          </div>
        </template>
        <template #footer="{ close }">
          <UButton color="neutral" variant="outline" label="キャンセル" :disabled="deleting" @click="close" />
          <UButton color="error" icon="i-lucide-trash-2" label="削除" :loading="deleting" @click="confirmDelete" />
        </template>
      </UModal>
    </template>
  </UDashboardPanel>
</template>
