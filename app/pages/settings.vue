<script setup lang="ts">
import type { FormError, FormSubmitEvent } from '@nuxt/ui'
import type { AppSettings } from '#shared/types'
import { DEFAULT_SETTINGS } from '#shared/types'

useHead({ title: '設定 - Phrapedia' })

const toast = useToast()
const { isDesktop } = useDesktop()
const { data: view, status, error, refresh, save, reindex, ready } = useSettings()
await ready

const state = reactive<AppSettings>(toAppSettings(DEFAULT_SETTINGS))
const loaded = ref(false)

watch(view, (v) => {
  if (!v) return
  Object.assign(state, toAppSettings(v))
  loaded.value = true
}, { immediate: true })

const engineItems = [
  { label: 'Antigravity CLI（agy）— 推奨', value: 'antigravity' },
  { label: 'Gemini CLI（gemini）— 旧 CLI', value: 'gemini-cli' },
]

// Keep the command in sync with the engine while the user has not customised it
watch(() => state.gemini.engine, (engine, prev) => {
  const defaults: Record<string, string> = { 'antigravity': 'agy', 'gemini-cli': 'gemini' }
  if (!prev || state.gemini.command === defaults[prev]) state.gemini.command = defaults[engine] ?? state.gemini.command
})

const argsText = computed({
  get: () => state.gemini.args.join('\n'),
  set: (v: string) => { state.gemini.args = v.split('\n').map(s => s.trim()).filter(Boolean) },
})

function validate(s: AppSettings): FormError[] {
  const errors: FormError[] = []
  if (!s.sourcesDir?.trim()) errors.push({ name: 'sourcesDir', message: '一次資料フォルダを入力してください' })
  if (!s.dataDir?.trim()) errors.push({ name: 'dataDir', message: 'データフォルダを入力してください' })
  if (!s.ai.anthropicModel?.trim()) errors.push({ name: 'ai.anthropicModel', message: 'モデル名を入力してください' })
  if (!Number.isFinite(s.ai.maxTokens) || s.ai.maxTokens < 256) errors.push({ name: 'ai.maxTokens', message: '256 以上の数値を入力してください' })
  if (!s.gemini.command?.trim()) errors.push({ name: 'gemini.command', message: 'コマンドを入力してください' })
  if (!Number.isFinite(s.gemini.timeoutMs) || s.gemini.timeoutMs < 1000) errors.push({ name: 'gemini.timeoutMs', message: '1000 ms 以上を指定してください' })
  if (!Number.isFinite(s.gemini.cacheHours) || s.gemini.cacheHours < 0) errors.push({ name: 'gemini.cacheHours', message: '0 以上を指定してください' })
  if (s.git.authorEmail && !/^[^\s@]+@[^\s@]+$/.test(s.git.authorEmail)) errors.push({ name: 'git.authorEmail', message: 'メールアドレスの形式が正しくありません' })
  return errors
}

const saving = ref(false)
async function onSubmit(_e: FormSubmitEvent<AppSettings>) {
  saving.value = true
  try {
    await save(toAppSettings(state))
    toast.add({ title: '設定を保存しました', icon: 'i-lucide-check', color: 'success' })
  } catch (err) {
    toast.add({ title: '保存に失敗しました', description: errorMessage(err), icon: 'i-lucide-circle-alert', color: 'error' })
  } finally {
    saving.value = false
  }
}

function resetForm() {
  if (view.value) Object.assign(state, toAppSettings(view.value))
}

const reindexing = ref(false)
async function onReindex() {
  reindexing.value = true
  try {
    const res = await reindex()
    toast.add({ title: '再インデックスしました', description: `全 ${res.total} 件（更新 ${res.indexed}・削除 ${res.removed}）`, icon: 'i-lucide-check', color: 'success' })
  } catch (err) {
    toast.add({ title: '再インデックスに失敗しました', description: errorMessage(err), icon: 'i-lucide-circle-alert', color: 'error' })
  } finally {
    reindexing.value = false
  }
}
</script>

<template>
  <UDashboardPanel id="settings">
    <template #header>
      <UDashboardNavbar title="設定">
        <template #right>
          <UButton v-if="loaded" color="neutral" variant="ghost" label="元に戻す" icon="i-lucide-undo-2" :disabled="saving" @click="resetForm" />
          <UButton v-if="loaded" form="settings-form" type="submit" label="保存" icon="i-lucide-save" :loading="saving" />
        </template>
      </UDashboardNavbar>
    </template>

    <template #body>
      <div class="mx-auto w-full max-w-3xl">
        <DesktopPathsCard v-if="isDesktop" class="mb-6" />

        <div v-if="status === 'pending' && !loaded" class="space-y-4" aria-busy="true">
          <USkeleton v-for="i in 4" :key="i" class="h-40 w-full" />
        </div>

        <UAlert
          v-else-if="error && !loaded"
          color="error"
          variant="subtle"
          icon="i-lucide-circle-alert"
          title="設定を読み込めませんでした"
          :description="errorMessage(error)"
          :actions="[{ label: '再試行', color: 'neutral', variant: 'outline', onClick: () => { refresh() } }]"
        />

        <UForm v-else id="settings-form" :state="state" :validate="validate" class="space-y-6" @submit="onSubmit">
          <!-- 一次資料 -->
          <UCard>
            <template #header>
              <div class="flex items-center justify-between gap-2">
                <h2 class="font-semibold text-highlighted flex items-center gap-2"><UIcon name="i-lucide-library" class="size-4" />一次資料</h2>
                <UButton size="sm" color="neutral" variant="outline" icon="i-lucide-refresh-cw" label="再インデックス" :loading="reindexing" @click="onReindex" />
              </div>
            </template>
            <div class="space-y-4">
              <UFormField label="一次資料フォルダ" name="sourcesDir" description="絶対パス、またはプロジェクトからの相対パス" required>
                <UInput v-model="state.sourcesDir" class="w-full" placeholder="sources" />
                <template #help>
                  <span v-if="view?.resolved" class="font-mono text-xs text-muted break-all">{{ view.resolved.sourcesDir }}</span>
                </template>
              </UFormField>
              <UFormField label="データフォルダ" name="dataDir" description="data/<種類>/<slug>.md を保存する場所（Git 管理）" required>
                <UInput v-model="state.dataDir" class="w-full" placeholder="data" />
                <template #help>
                  <span v-if="view?.resolved" class="font-mono text-xs text-muted break-all">{{ view.resolved.dataDir }}</span>
                </template>
              </UFormField>
            </div>
          </UCard>

          <!-- AI -->
          <UCard>
            <template #header>
              <div class="flex flex-wrap items-center justify-between gap-2">
                <h2 class="font-semibold text-highlighted flex items-center gap-2"><UIcon name="i-lucide-sparkles" class="size-4" />AI</h2>
                <div v-if="view" class="flex flex-wrap gap-1.5">
                  <UBadge
                    :label="`Anthropic API キー ${view.capabilities.anthropicConfigured ? '設定済み' : '未設定'}`"
                    :color="view.capabilities.anthropicConfigured ? 'success' : 'neutral'"
                    :icon="view.capabilities.anthropicConfigured ? 'i-lucide-key-round' : 'i-lucide-circle-slash'"
                    variant="subtle"
                  />
                  <UBadge
                    :label="`${view.capabilities.geminiEngine} ${view.capabilities.geminiFound ? '検出済み' : '未検出'}${view.capabilities.geminiVersion ? ` (${view.capabilities.geminiVersion})` : ''}`"
                    :color="view.capabilities.geminiFound ? 'success' : 'neutral'"
                    :icon="view.capabilities.geminiFound ? 'i-lucide-terminal' : 'i-lucide-circle-slash'"
                    variant="subtle"
                  />
                </div>
              </div>
            </template>
            <div class="space-y-4">
              <p class="text-sm text-muted">チャット AI は Claude（Anthropic API）です。API キーは環境変数 ANTHROPIC_API_KEY（または NUXT_ANTHROPIC_API_KEY）で設定します。Gemini はインターネット検索（Web 調査ツール）にのみ使われます。</p>
              <div class="grid gap-4 sm:grid-cols-2">
                <UFormField label="Anthropic モデル" name="ai.anthropicModel" required>
                  <UInput v-model="state.ai.anthropicModel" class="w-full font-mono" placeholder="claude-sonnet-5-5" />
                </UFormField>
                <UFormField label="最大トークン数" name="ai.maxTokens">
                  <UInput v-model.number="state.ai.maxTokens" type="number" min="256" step="256" class="w-full" />
                </UFormField>
              </div>
              <UFormField label="追加の指示" name="ai.extraInstructions" description="システムプロンプトの末尾に追加されます">
                <UTextarea v-model="state.ai.extraInstructions" :rows="4" autoresize class="w-full" placeholder="例: 年号は西暦と仏暦を併記する" />
              </UFormField>
            </div>
          </UCard>

          <!-- Google CLI (Antigravity / Gemini) -->
          <UCard>
            <template #header>
              <h2 class="font-semibold text-highlighted flex items-center gap-2"><UIcon name="i-lucide-terminal" class="size-4" />Google CLI（Web 調査）</h2>
            </template>
            <div class="space-y-4">
              <UFormField label="CLI" name="gemini.engine" description="2026 年 6 月以降、無料枠 / Google One では Gemini CLI は Antigravity CLI（agy）に置き換わっています。agy は curl -fsSL https://antigravity.google/cli/install.sh | bash で導入">
                <USelect v-model="state.gemini.engine" :items="engineItems" class="w-full sm:w-96" />
              </UFormField>
              <div class="grid gap-4 sm:grid-cols-2">
                <UFormField label="コマンド" name="gemini.command" required description="PATH が通らない場合は絶対パス（例: ~/.local/bin/agy）">
                  <UInput v-model="state.gemini.command" class="w-full font-mono" :placeholder="state.gemini.engine === 'gemini-cli' ? 'gemini' : 'agy'" />
                </UFormField>
                <UFormField label="モデル" name="gemini.model" description="空欄 = CLI の既定（agy: --model / gemini: -m）">
                  <UInput v-model="state.gemini.model" class="w-full font-mono" placeholder="例: flash" />
                </UFormField>
              </div>
              <p class="text-sm text-muted">承認モードは <code class="font-mono text-xs">auto</code> に固定です（agy: <code class="font-mono text-xs">--mode auto</code> / gemini: <code class="font-mono text-xs">--approval-mode auto_edit</code>）。変更が必要な場合のみ追加引数で上書きしてください。</p>
              <UFormField label="追加引数" name="gemini.args" description="1 行に 1 つ">
                <UTextarea v-model="argsText" :rows="3" autoresize class="w-full" :ui="{ base: 'font-mono text-sm' }" />
              </UFormField>
              <div class="grid gap-4 sm:grid-cols-2">
                <UFormField label="タイムアウト（ms）" name="gemini.timeoutMs">
                  <UInput v-model.number="state.gemini.timeoutMs" type="number" min="1000" step="1000" class="w-full" />
                </UFormField>
                <UFormField label="キャッシュ（時間）" name="gemini.cacheHours" description="0 = キャッシュしない">
                  <UInput v-model.number="state.gemini.cacheHours" type="number" min="0" class="w-full" />
                </UFormField>
              </div>
            </div>
          </UCard>

          <!-- Git -->
          <UCard>
            <template #header>
              <h2 class="font-semibold text-highlighted flex items-center gap-2"><UIcon name="i-lucide-git-branch" class="size-4" />Git 同期</h2>
            </template>
            <div class="space-y-4">
              <div class="grid gap-4 sm:grid-cols-2">
                <UFormField label="リモート" name="git.remote">
                  <UInput v-model="state.git.remote" class="w-full font-mono" placeholder="origin" />
                </UFormField>
                <UFormField label="ブランチ" name="git.branch" description="空欄 = 現在のブランチ">
                  <UInput v-model="state.git.branch" class="w-full font-mono" placeholder="main" />
                </UFormField>
                <UFormField label="コミット作成者名" name="git.authorName">
                  <UInput v-model="state.git.authorName" class="w-full" />
                </UFormField>
                <UFormField label="コミット作成者メール" name="git.authorEmail">
                  <UInput v-model="state.git.authorEmail" type="email" class="w-full" />
                </UFormField>
              </div>
              <USwitch v-model="state.git.autoCommit" label="保存ごとに自動コミット" description="レコードの保存・削除のたびにコミットします" />
              <USwitch v-model="state.git.autoPush" :disabled="!state.git.autoCommit" label="自動コミット後にプッシュ" description="自動コミットが有効なときのみ" />
            </div>
          </UCard>

          <div class="flex justify-end gap-2 pb-4">
            <UButton color="neutral" variant="ghost" label="元に戻す" :disabled="saving" @click="resetForm" />
            <UButton type="submit" label="保存" icon="i-lucide-save" :loading="saving" />
          </div>
        </UForm>
      </div>
    </template>
  </UDashboardPanel>
</template>
