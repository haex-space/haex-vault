<script setup lang="ts">
useHead({ title: 'Holzi' })

// Spec 060 FR-018a: a link of a share is parked until a vault is open.
const deepLink = useDeepLink()
onMounted(() => {
  void deepLink.init()
})
</script>

<template>
  <div>
    <NuxtPage />
    <ShadcnSonnerToaster position="bottom-right" />
  </div>
</template>
