import type { SyncResult, SyncStatus } from '#shared/types'

export type SyncAction = SyncResult['action']

const ACTION_LABELS: Record<SyncAction, string> = {
  commit: 'コミット',
  pull: 'プル',
  push: 'プッシュ',
  sync: '同期',
}

export function useSync() {
  const status = useAsyncData<SyncStatus>('sync-status', () => api.syncStatus(), { dedupe: 'defer' })
  const running = useState<SyncAction | null>('sync-running', () => null)
  const toast = useToast()

  async function run(action: SyncAction, message?: string): Promise<SyncResult | undefined> {
    if (running.value) return
    running.value = action
    try {
      const res = await api.syncRun(action, action === 'commit' || action === 'sync' ? message?.trim() || undefined : undefined)
      toast.add({
        title: res.ok ? `${ACTION_LABELS[action]}しました` : `${ACTION_LABELS[action]}に失敗しました`,
        description: res.message,
        color: res.ok ? 'success' : 'error',
        icon: res.ok ? 'i-lucide-check' : 'i-lucide-circle-alert',
      })
      if (res.status) status.data.value = res.status
      await status.refresh()
      return res
    } catch (err) {
      toast.add({ title: `${ACTION_LABELS[action]}に失敗しました`, description: errorMessage(err), color: 'error', icon: 'i-lucide-circle-alert' })
      await status.refresh()
    } finally {
      running.value = null
    }
  }

  const changeCount = computed(() => status.data.value?.changes?.length ?? 0)

  return { ...status, running, run, changeCount, actionLabels: ACTION_LABELS }
}
