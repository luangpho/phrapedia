<script setup lang="ts">
const { data: status, error, status: fetchStatus, refresh, running, run, changeCount } = useSync()
const message = ref('')

const loading = computed(() => fetchStatus.value === 'pending' && !status.value)

const STATUS_LABEL: Record<string, string> = { 'M': '変更', 'A': '追加', 'D': '削除', 'R': '名前変更', '??': '未追跡', 'U': '競合' }
function statusLabel(code: string) {
  const c = code.trim()
  return STATUS_LABEL[c] ?? STATUS_LABEL[c[0] ?? ''] ?? c
}
function statusColor(code: string): 'success' | 'warning' | 'error' | 'neutral' | 'info' {
  const c = code.trim()
  if (c === '??' || c.startsWith('A')) return 'success'
  if (c.startsWith('D')) return 'error'
  if (c.startsWith('U')) return 'error'
  if (c.startsWith('M') || c.startsWith('R')) return 'warning'
  return 'neutral'
}

async function doCommit() {
  const res = await run('commit', message.value)
  if (res?.ok) message.value = ''
}
async function doSync() {
  const res = await run('sync', message.value)
  if (res?.ok) message.value = ''
}

function formatDate(d?: string) {
  if (!d) return ''
  const t = new Date(d)
  return Number.isNaN(t.getTime()) ? d : t.toLocaleString('ja-JP')
}
</script>

<template>
  <div class="space-y-4">
    <div v-if="loading" class="space-y-2" aria-busy="true">
      <USkeleton class="h-4 w-1/2" />
      <USkeleton class="h-4 w-2/3" />
      <USkeleton class="h-24 w-full" />
    </div>

    <UAlert
      v-else-if="error"
      color="error"
      variant="subtle"
      icon="i-lucide-circle-alert"
      title="状態を取得できませんでした"
      :description="errorMessage(error)"
      :actions="[{ label: '再試行', color: 'neutral', variant: 'outline', onClick: () => { refresh() } }]"
    />

    <UAlert
      v-else-if="status && !status.isRepo"
      color="warning"
      variant="subtle"
      icon="i-lucide-git-branch"
      title="Git リポジトリではありません"
      :description="status.error || 'データディレクトリが Git 管理されていないため同期できません。git init とリモート設定を行ってください。'"
    />

    <template v-else-if="status">
      <dl class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 text-sm">
        <dt class="text-muted">ブランチ</dt>
        <dd class="text-highlighted font-mono">{{ status.branch || '-' }}</dd>
        <dt class="text-muted">リモート</dt>
        <dd class="text-highlighted font-mono break-all">{{ status.remote || '-' }}<span v-if="status.remoteUrl" class="text-muted"> ({{ status.remoteUrl }})</span></dd>
        <dt class="text-muted">差分</dt>
        <dd class="flex gap-2">
          <UBadge :label="`↑ ${status.ahead} 先行`" color="neutral" variant="outline" size="sm" />
          <UBadge :label="`↓ ${status.behind} 遅れ`" color="neutral" variant="outline" size="sm" />
        </dd>
        <template v-if="status.lastCommit">
          <dt class="text-muted">最終コミット</dt>
          <dd class="min-w-0">
            <span class="font-mono text-xs text-muted">{{ status.lastCommit.hash.slice(0, 7) }}</span>
            <span class="text-highlighted"> {{ status.lastCommit.message }}</span>
            <span class="block text-xs text-muted">{{ status.lastCommit.author }} ・ {{ formatDate(status.lastCommit.date) }}</span>
          </dd>
        </template>
      </dl>

      <UAlert v-if="status.error" color="warning" variant="subtle" icon="i-lucide-triangle-alert" :description="status.error" />

      <div>
        <h3 class="text-xs font-medium text-muted mb-1.5">変更されたファイル（{{ changeCount }}）</h3>
        <ul v-if="status.changes.length" class="max-h-56 overflow-y-auto rounded-md border border-default divide-y divide-default text-sm">
          <li v-for="c in status.changes" :key="c.path" class="flex items-center gap-2 px-3 py-1.5">
            <UBadge :label="statusLabel(c.status)" :color="statusColor(c.status)" variant="subtle" size="xs" class="shrink-0" />
            <span class="font-mono text-xs truncate" :title="c.path">{{ c.path }}</span>
          </li>
        </ul>
        <p v-else class="text-sm text-muted">変更はありません</p>
      </div>

      <UFormField label="コミットメッセージ" name="commit-message" hint="空欄なら自動生成">
        <UInput v-model="message" placeholder="data: 高僧情報を更新" class="w-full" :disabled="!!running" @keydown.enter.prevent="doCommit" />
      </UFormField>

      <div class="flex flex-wrap gap-2">
        <UButton icon="i-lucide-git-commit-horizontal" label="コミット" color="neutral" variant="outline" :loading="running === 'commit'" :disabled="!!running || !changeCount" @click="doCommit" />
        <UButton icon="i-lucide-arrow-down-to-line" label="プル" color="neutral" variant="outline" :loading="running === 'pull'" :disabled="!!running" @click="run('pull')" />
        <UButton icon="i-lucide-arrow-up-from-line" label="プッシュ" color="neutral" variant="outline" :loading="running === 'push'" :disabled="!!running" @click="run('push')" />
        <UButton icon="i-lucide-refresh-cw" label="同期" :loading="running === 'sync'" :disabled="!!running" class="ms-auto" @click="doSync" />
      </div>
      <p class="text-xs text-muted">同期 = コミット → pull --rebase → push</p>
    </template>
  </div>
</template>
