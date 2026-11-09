import { describe, it, expect } from 'vitest'
import { execFileSync } from 'node:child_process'
import { copyFileSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const script = join(__dirname, '..', 'packaging', 'homebrew', 'update-cask.mjs')
const template = join(__dirname, '..', 'packaging', 'homebrew', 'Casks', 'phrapedia.rb')
const ARM = 'a'.repeat(64)
const INTEL = 'b'.repeat(64)

describe('update-cask.mjs with the current cask template', () => {
  it('rewrites version and both sha256 and keeps the rest', () => {
    const dir = mkdtempSync(join(tmpdir(), 'cask-test-'))
    try {
      const cask = join(dir, 'phrapedia.rb')
      copyFileSync(template, cask)
      execFileSync('node', [script, '--version', 'v1.2.3', '--arm-sha', ARM, '--intel-sha', INTEL, '--cask', cask])
      const out = readFileSync(cask, 'utf8')
      expect(out).toContain('version "1.2.3"')
      expect(out).toContain(`sha256 arm:   "${ARM}"`)
      expect(out).toContain(`intel: "${INTEL}"`)
      expect(out).toContain('auto_updates true')
      expect(out).toContain('preflight_steps do')
      expect(out).toContain('"-dr", "com.apple.quarantine", "{{staged_path}}/Phrapedia.app"')
      expect(out.indexOf('app "Phrapedia.app"')).toBeLessThan(out.indexOf('preflight_steps'))
      expect(out.indexOf('preflight_steps')).toBeLessThan(out.indexOf('zap trash'))
    } finally {
      rmSync(dir, { recursive: true, force: true })
    }
  })

  it('computes sha256 from dmg files', () => {
    const dir = mkdtempSync(join(tmpdir(), 'cask-test-'))
    try {
      const cask = join(dir, 'phrapedia.rb')
      copyFileSync(template, cask)
      writeFileSync(join(dir, 'a.dmg'), 'arm')
      writeFileSync(join(dir, 'b.dmg'), 'intel')
      execFileSync('node', [script, '--version', '0.1.0', '--arm-dmg', join(dir, 'a.dmg'), '--intel-dmg', join(dir, 'b.dmg'), '--cask', cask])
      const out = readFileSync(cask, 'utf8')
      expect(out).toContain('version "0.1.0"')
      expect(out).not.toContain('0'.repeat(64))
    } finally {
      rmSync(dir, { recursive: true, force: true })
    }
  })
})
