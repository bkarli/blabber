<script setup lang="ts">
import type { HTMLAttributes } from 'vue'
import { CollapsibleTrigger, type CollapsibleTriggerProps } from 'reka-ui'
import { reactiveOmit } from '@vueuse/core'
import { cn } from '@/lib/utils'

const props = defineProps<CollapsibleTriggerProps & { class?: HTMLAttributes['class'] }>()

const delegatedProps = reactiveOmit(props, 'class')
</script>

<template>
  <CollapsibleTrigger data-slot="collapsible-trigger" v-bind="delegatedProps" :class="cn(props.class)">
    <slot />
  </CollapsibleTrigger>
</template>
