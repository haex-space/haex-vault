<template>
  <HaexSystem
    :is-dragging="isDragging"
    disable-content-scroll
  >
    <UDashboardGroup
      :persistent="false"
      class="h-full"
    >
      <UDashboardSidebar
        resizable
        :min-size="15"
        :max-size="40"
        :default-size="22"
      >
        <HaexSystemPasswordsSidebar />
      </UDashboardSidebar>
      <UDashboardPanel>
        <HaexSystemPasswordsHeader />
        <!-- content-switch lands in Schritt 1.5 -->
      </UDashboardPanel>
    </UDashboardGroup>
  </HaexSystem>
</template>

<script setup lang="ts">
defineProps<{
  tabId?: string
  isDragging?: boolean
}>()
</script>
