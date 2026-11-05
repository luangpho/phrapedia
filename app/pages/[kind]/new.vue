<script setup lang="ts">
import type { EntityKind } from '#shared/types'

definePageMeta({
  validate: route => ['monks', 'temples', 'amulets'].includes(String(route.params.kind)),
})

const route = useRoute()
const kind = computed(() => String(route.params.kind) as EntityKind)
const initialName = computed(() => (typeof route.query.name === 'string' ? route.query.name : ''))

const toast = useToast()
const { create } = useEntities()

const { data: tpl, status, error, refresh } = await useFetch<{ raw: string }>(
  () => `/api/entities/${kind.value}/template`,
  { key: computed(() => `entity-template:${kind.value}:${initialName.value}`), query: computed(() => (initialName.value ? { name: initialName.value } : {})) },
)

const raw = ref(tpl.value?.raw ?? '')
const baseline = ref(tpl.value?.raw ?? '')
watch(() => tpl.value?.raw, (v) => {
  if (v == null) return
  // Replace the template only while the user has not typed anything yet.
  if (raw.value === baseline.value) raw.value = v
  baseline.value = v
})

const dirty = computed(() => raw.value !== baseline.value)
const saving = ref(false)
let skipGuard = false

useUnsavedGuard(computed(() => dirty.value && !skipGuard))

const title = computed(() => `${kindLabel(kind.value)}を追加`)
useHead({ title: () => `${title.value} - Phrapedia` })

const hint = computed(() => kind.value === 'temples'
  ? 'チャットで寺院名などを伝えると、一次資料とWeb調査をもとに下書きを作成します'
  : kind.value === 'monks'
    ? 'チャットで高僧名などを伝えると、一次資料とWeb調査をもとに下書きを作成します'
    : 'チャットで名称などを伝えると、一次資料とWeb調査をもとに下書きを作成します')

// Chat is open by default (on small screens it would cover the editor, so it starts closed there).
const chatOpen = ref(true)
onMounted(() => {
  if (!window.matchMedia('(min-width: 1024px)').matches) chatOpen.value = false
})

const draftNotice = ref<string | null>(null)
function onDraft(markdown: string, summary?: string) {
  raw.value = markdown
  draftNotice.value = summary || ''
}

/** The AI saved the record itself (save_entity): open the saved record instead of saving a duplicate. */
async function onSaved(saved: { kind: EntityKind; slug: string; path?: string }) {
  skipGuard = true
  baseline.value = raw.value
  await bindDraftChatSession(kind.value, saved.slug)
  await useEntities().refreshList(saved.kind)
  toast.add({ title: 'AI がレコードを保存しました', description: saved.path, icon: 'i-lucide-check', color: 'success' })
  await navigateTo(`/${saved.kind}/${saved.slug}`)
}

/** A previous draft conversation was restored: bring back its last draft while the editor is still untouched. */
function onRestored(last: { markdown: string; summary?: string } | null) {
  if (!last || raw.value !== baseline.value) return
  raw.value = last.markdown
  draftNotice.value = last.summary || '前回の下書きを復元しました'
}

async function save() {
  if (saving.value) return
  if (!raw.value.trim()) return
  saving.value = true
  try {
    const res = await create(kind.value, raw.value)
    await bindDraftChatSession(kind.value, res.record.slug)
    toast.add({ title: '保存しました', description: res.record.path, icon: 'i-lucide-check', color: 'success' })
    skipGuard = true
    baseline.value = raw.value
    await navigateTo(`/${kind.value}/${res.record.slug}`)
  } catch (err) {
    toast.add({ title: '保存に失敗しました', description: errorMessage(err), icon: 'i-lucide-circle-alert', color: 'error' })
  } finally {
    saving.value = false
  }
}

const discardOpen = ref(false)
async function discard() {
  if (dirty.value) {
    discardOpen.value = true
    return
  }
  await navigateTo('/')
}
async function confirmDiscard() {
  skipGuard = true
  discardOpen.value = false
  raw.value = baseline.value
  clearDraftChatSession(kind.value)
  await navigateTo('/')
}
</script>

<template>
  <UDashboardPanel :id="`new-${kind}`" :ui="{ body: 'p-0 sm:p-0 gap-0 sm:gap-0 overflow-hidden' }">
    <template #header>
      <UDashboardNavbar :ui="{ right: 'gap-1.5' }">
        <template #title>
          <span class="flex items-center gap-2">
            <UIcon :name="kindIcon(kind)" class="size-5 text-muted" />
            {{ title }}
            <UBadge v-if="dirty" label="未保存" color="warning" variant="soft" size="sm" icon="i-lucide-circle-dot" />
          </span>
        </template>
        <template #right>
          <UButton color="neutral" variant="ghost" icon="i-lucide-x" aria-label="破棄" :disabled="saving" @click="discard">
            <span class="hidden md:inline">破棄</span>
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
          <UButton icon="i-lucide-save" aria-label="保存" :loading="saving" :disabled="!raw.trim() || status === 'pending'" @click="save">
            <span class="hidden md:inline">保存</span>
          </UButton>
        </template>
      </UDashboardNavbar>
    </template>

    <template #body>
      <EntityWorkspace v-model:open="chatOpen" :kind="kind" :slug="null" :draft="raw" :hint="hint" @draft="onDraft" @saved="onSaved" @restored="onRestored">
        <div class="max-w-4xl space-y-4">
          <UAlert
            v-if="error"
            color="error"
            variant="subtle"
            icon="i-lucide-circle-alert"
            title="テンプレートを取得できませんでした"
            :description="errorMessage(error)"
            :actions="[{ label: '再試行', color: 'neutral', variant: 'outline', onClick: () => { refresh() } }]"
          />
          <UAlert
            v-if="draftNotice !== null"
            color="primary"
            variant="subtle"
            icon="i-lucide-sparkles"
            title="AIが下書きを更新しました"
            :description="draftNotice || '内容を確認して保存してください。'"
            close
            @update:open="(v: boolean) => { if (!v) draftNotice = null }"
          />
          <p class="text-sm text-muted">
            先頭の <code class="font-mono text-xs">---</code> で囲まれた部分（frontmatter）に名前・タイ語名・状態などを書きます。
            slug を空にすると英語名または名前から自動で決まります。
          </p>
          <div v-if="status === 'pending' && !raw" class="space-y-2" aria-busy="true">
            <USkeleton class="h-8 w-48" />
            <USkeleton class="h-96 w-full" />
          </div>
          <MarkdownEditor v-else v-model="raw" :disabled="saving" @save="save" />
        </div>
      </EntityWorkspace>

      <UModal v-model:open="discardOpen" title="下書きを破棄しますか？" description="保存されていない内容は失われます。" :ui="{ footer: 'justify-end' }">
        <template #footer="{ close }">
          <UButton color="neutral" variant="outline" label="編集を続ける" @click="close" />
          <UButton color="error" icon="i-lucide-trash-2" label="破棄する" @click="confirmDiscard" />
        </template>
      </UModal>
    </template>
  </UDashboardPanel>
</template>
