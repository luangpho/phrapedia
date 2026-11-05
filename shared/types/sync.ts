export interface SyncFileChange {
  path: string
  /** porcelain status code, e.g. 'M', 'A', 'D', '??' */
  status: string
}

export interface SyncStatus {
  isRepo: boolean
  branch?: string
  remote?: string
  remoteUrl?: string
  ahead: number
  behind: number
  changes: SyncFileChange[]
  lastCommit?: { hash: string; message: string; date: string; author: string }
  error?: string
}

export interface SyncResult {
  ok: boolean
  action: 'commit' | 'push' | 'pull' | 'sync'
  message: string
  commitHash?: string
  status?: SyncStatus
}
