<script setup lang="ts">
/**
 * The trash of Wallet (spec 092, Phase W): the cards and tickets that were deleted, with "Restore"
 * and "Delete for good". It is the view of the password manager's trash for passes; the two apps
 * share one trash and neither restores or empties what the other shows.
 */
</script>

<template>
  <PasswordsTrashView scope="pass" />
</template>
