<script setup lang="ts">
import type { NuxtError } from '#app'

const props = defineProps<{ error: NuxtError }>()

const title = computed(() => (props.error.statusCode === 404 ? 'ページが見つかりません' : 'エラーが発生しました'))

useHead({ title: () => `${title.value} - Phrapedia` })

function goHome() {
  clearError({ redirect: '/' })
}
</script>

<template>
  <UApp>
    <main class="min-h-svh flex items-center justify-center p-6 bg-default">
      <div class="max-w-md text-center space-y-4">
        <p class="text-5xl font-semibold text-primary">{{ error.statusCode }}</p>
        <h1 class="text-xl font-semibold text-highlighted">{{ title }}</h1>
        <p class="text-sm text-muted break-words">{{ error.message || error.statusMessage }}</p>
        <UButton icon="i-lucide-house" label="ホームへ戻る" @click="goHome" />
      </div>
    </main>
  </UApp>
</template>
