<script setup lang="ts">
useHead({ title: 'Phrapedia' })

const monks = useEntityList('monks')
const temples = useEntityList('temples')

const cards = computed(() => [
  { kind: 'monks' as const, list: monks },
  { kind: 'temples' as const, list: temples },
])

const requestUrl = useRequestURL()
const mcpUrl = computed(() => `${requestUrl.origin}/mcp`)
const mcpJson = computed(() => JSON.stringify({ mcpServers: { phrapedia: { httpUrl: mcpUrl.value } } }, null, 2))

const toast = useToast()
async function copyMcp() {
  try {
    await navigator.clipboard.writeText(mcpJson.value)
    toast.add({ title: 'コピーしました', icon: 'i-lucide-clipboard-check', color: 'success' })
  } catch {
    toast.add({ title: 'コピーできませんでした', color: 'error', icon: 'i-lucide-circle-alert' })
  }
}
</script>

<template>
  <UDashboardPanel id="home">
    <template #header>
      <UDashboardNavbar title="ホーム" />
    </template>

    <template #body>
      <div class="mx-auto w-full max-w-3xl space-y-8">
        <section class="space-y-2">
          <h2 class="text-2xl font-semibold text-highlighted tracking-tight">Phrapedia</h2>
          <p class="text-muted leading-7">
            タイの高僧・寺院・プラクルアン（お守り）の情報を Markdown ファイルとして蓄積するナレッジベースです。
            手元の一次資料を最優先に、AI と Web 調査で下書きを作り、確認しながら育てていきます。
          </p>
        </section>

        <section class="grid gap-4 sm:grid-cols-2" aria-label="登録件数">
          <UCard v-for="c in cards" :key="c.kind" :ui="{ body: 'p-4 sm:p-5' }">
            <div class="flex items-start justify-between gap-3">
              <div>
                <p class="text-sm text-muted flex items-center gap-1.5">
                  <UIcon :name="kindIcon(c.kind)" class="size-4" />{{ kindLabel(c.kind) }}
                </p>
                <USkeleton v-if="c.list.status.value === 'pending' && !c.list.data.value?.length" class="mt-2 h-8 w-12" />
                <p v-else-if="c.list.error.value" class="mt-2 text-sm text-error">取得できませんでした</p>
                <p v-else class="mt-1 text-3xl font-semibold text-highlighted tabular-nums">
                  {{ c.list.data.value?.length ?? 0 }}<span class="text-base font-normal text-muted ms-1">件</span>
                </p>
              </div>
              <UButton :to="`/${c.kind}/new`" icon="i-lucide-plus" :label="`${kindLabel(c.kind)}を追加`" size="sm" />
            </div>
          </UCard>
        </section>

        <section class="space-y-3">
          <h3 class="text-sm font-semibold text-highlighted">使い方</h3>
          <ol class="list-decimal ps-5 space-y-1.5 text-sm text-muted leading-6">
            <li>「高僧を追加」から、チャットで名前を伝えると一次資料と Web 調査をもとに下書きが作られます。</li>
            <li>内容を確認・修正して保存すると <code class="font-mono text-xs">data/&lt;種類&gt;/&lt;slug&gt;.md</code> に書き出されます。</li>
            <li>サイドバー下部の Git 同期から GitHub と共有できます。</li>
          </ol>
        </section>

        <section class="space-y-3">
          <div class="flex items-center justify-between gap-2">
            <h3 class="text-sm font-semibold text-highlighted flex items-center gap-1.5">
              <UIcon name="i-lucide-plug" class="size-4" />MCP エンドポイント
            </h3>
            <UButton icon="i-lucide-copy" size="xs" color="neutral" variant="ghost" label="設定をコピー" @click="copyMcp" />
          </div>
          <p class="text-sm text-muted leading-6">
            Claude Code や Antigravity CLI から <code class="font-mono text-xs text-highlighted">{{ mcpUrl }}</code> に接続すると、
            同じデータを MCP ツール（検索・取得・保存・一次資料・Web 調査・同期）で扱えます。
          </p>
          <pre class="rounded-md border border-default bg-elevated/50 p-3 text-xs font-mono overflow-x-auto">{{ mcpJson }}</pre>
          <p class="text-sm text-muted">
            一次資料フォルダや AI プロバイダは <ULink to="/settings" class="text-primary hover:underline">設定</ULink> で変更できます。
            資料の一覧は <ULink to="/sources" class="text-primary hover:underline">一次資料</ULink> から確認できます。
          </p>
        </section>
      </div>
    </template>
  </UDashboardPanel>
</template>
