<script setup lang="ts">
import { CollapsibleRoot, type CollapsibleRootEmits, type CollapsibleRootProps } from 'reka-ui'

const props = defineProps<CollapsibleRootProps>()
const emits = defineEmits<CollapsibleRootEmits>()
</script>

<template>
  <CollapsibleRoot data-slot="collapsible" v-bind="props" @update:open="emits('update:open', $event)">
    <slot />
  </CollapsibleRoot>
</template>
