import { Marked } from 'marked'
import type { Token } from 'marked'
import DOMPurify from 'dompurify'

const FRONTMATTER_RE = /^﻿?---\r?\n[\s\S]*?\r?\n---[ \t]*(?:\r?\n|$)/

/** Remove a leading YAML frontmatter block, if any. */
export function stripFrontmatter(md: string): string {
  return md.replace(FRONTMATTER_RE, '')
}

let marked: Marked | undefined

function getMarked() {
  if (marked) return marked
  marked = new Marked({
    gfm: true,
    breaks: false,
    async: false,
    walkTokens(token: Token) {
      if (token.type === 'image') token.href = rewriteSourceHref(token.href)
      else if (token.type === 'link' && !isExternalUrl(token.href) && !token.href.startsWith('/') && !token.href.startsWith('#')) {
        // Relative links point into the primary sources: text files open in the sources browser, binaries are served raw.
        token.href = /\.(md|markdown|txt)$/i.test(token.href)
          ? `/sources?path=${encodeURIComponent(token.href.replace(/^\.\//, '').replace(/^sources\//, ''))}`
          : rewriteSourceHref(token.href)
      }
    },
  })
  return marked
}

/**
 * Markdown -> sanitized HTML. Only usable on the client (DOMPurify needs a DOM);
 * on the server it returns an empty string.
 */
export function useMarkdown() {
  function render(md: string, opts: { stripFrontmatter?: boolean } = {}): string {
    if (!import.meta.client) return ''
    const src = opts.stripFrontmatter === false ? md : stripFrontmatter(md ?? '')
    const html = getMarked().parse(src) as string
    return DOMPurify.sanitize(html, { ADD_ATTR: ['target', 'rel'] })
  }
  return { render, stripFrontmatter }
}
