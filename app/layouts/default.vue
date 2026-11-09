<script setup lang="ts">
import type { TabsItem } from '@nuxt/ui'
import type { EntityKind } from '#shared/types'

const open = ref(false) // mobile menu
const kind = useEntityKind()
const query = ref('')

const tabs: TabsItem[] = UI_KINDS.map(k => ({ label: kindLabel(k), value: k, icon: kindIcon(k) }))

const tabValue = computed({
  get: () => kind.value,
  set: (v: string | number) => { if (isEntityKind(v)) kind.value = v },
})

watch(kind, () => { query.value = '' })

const addLabel = computed(() => `${kindLabel(kind.value as EntityKind)}を追加`)
</script>

<template>
  <UDashboardGroup unit="rem" storage-key="phrapedia">
    <UDashboardSidebar
      id="main"
      v-model:open="open"
      collapsible
      resizable
      :min-size="14"
      :default-size="18"
      :max-size="28"
      class="bg-elevated/25"
      :ui="{ footer: 'lg:border-t lg:border-default flex-col items-stretch gap-0.5' }"
    >
      <template #header="{ collapsed }">
        <NuxtLink v-if="!collapsed" to="/" class="flex items-center gap-2 font-semibold text-highlighted rounded-md focus-visible:outline-2 focus-visible:outline-primary">
          <UIcon name="i-lucide-book-open-text" class="size-5 text-primary" />
          Phrapedia
        </NuxtLink>
        <div class="flex items-center gap-1" :class="collapsed ? 'mx-auto' : 'ms-auto'">
          <!-- collapsed: the header only fits one icon, so the update icon moves to the footer -->
          <UpdateButton v-if="!collapsed" />
          <UDashboardSidebarCollapse aria-label="サイドバーの折りたたみ" />
        </div>
      </template>

      <template #default="{ collapsed }">
        <template v-if="!collapsed">
          <UTabs v-model="tabValue" :items="tabs" :content="false" size="sm" class="w-full" aria-label="種類" />

          <UInput
            v-model="query"
            icon="i-lucide-search"
            :placeholder="`${kindLabel(kind)}を検索`"
            size="sm"
            class="w-full"
            :aria-label="`${kindLabel(kind)}を検索`"
          >
            <template v-if="query" #trailing>
              <UButton color="neutral" variant="link" size="xs" icon="i-lucide-x" aria-label="検索をクリア" @click="query = ''" />
            </template>
          </UInput>

          <UButton :to="`/${kind}/new`" icon="i-lucide-plus" :label="addLabel" block />

          <div class="-mx-2 flex-1 min-h-0">
            <EntityList :kind="kind" :query="query" @select="open = false" />
          </div>
        </template>
        <template v-else>
          <UTooltip v-for="k in UI_KINDS" :key="k" :text="kindLabel(k)">
            <UButton :icon="kindIcon(k)" color="neutral" :variant="kind === k ? 'soft' : 'ghost'" square :aria-label="kindLabel(k)" class="mx-auto" @click="kind = k" />
          </UTooltip>
          <UTooltip :text="addLabel">
            <UButton :to="`/${kind}/new`" icon="i-lucide-plus" square :aria-label="addLabel" class="mx-auto" />
          </UTooltip>
        </template>
      </template>

      <template #footer="{ collapsed }">
        <UpdateButton v-if="collapsed" class="justify-center" />
        <SyncStatusButton :collapsed="collapsed" />
        <UButton
          to="/sources"
          icon="i-lucide-library"
          :label="collapsed ? undefined : '一次資料'"
          aria-label="一次資料"
          color="neutral"
          variant="ghost"
          :square="collapsed"
          :block="!collapsed"
          class="justify-start"
          active-variant="soft"
        />
        <UButton
          to="/settings"
          icon="i-lucide-settings"
          :label="collapsed ? undefined : '設定'"
          aria-label="設定"
          color="neutral"
          variant="ghost"
          :square="collapsed"
          :block="!collapsed"
          class="justify-start"
          active-variant="soft"
        />
      </template>
    </UDashboardSidebar>

    <slot />
  </UDashboardGroup>
</template>
