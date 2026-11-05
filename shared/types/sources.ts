export type SourceFileKind = 'markdown' | 'text' | 'image' | 'pdf' | 'other'

export interface SourceFile {
  /** Path relative to the sources directory, POSIX separators */
  path: string
  name: string
  ext: string
  kind: SourceFileKind
  size: number
  /** ISO timestamp */
  mtime: string
}

export interface SourceSearchHit {
  path: string
  kind: SourceFileKind
  /** Text excerpt around the first match (markdown/text only) */
  snippet: string
  score: number
}

export interface SourceContent {
  path: string
  kind: SourceFileKind
  /** Text content for markdown/text files */
  text?: string
  /** Data URL-less base64 for images, with mimeType */
  base64?: string
  mimeType?: string
  size: number
}
