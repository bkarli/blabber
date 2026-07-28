<script setup lang="ts">
import type { HTMLAttributes } from 'vue'
import { CollapsibleContent, type CollapsibleContentProps } from 'reka-ui'
import { reactiveOmit } from '@vueuse/core'
import { cn } from '@/lib/utils'

const props = defineProps<CollapsibleContentProps & { class?: HTMLAttributes['class'] }>()

const delegatedProps = reactiveOmit(props, 'class')
</script>

<template>
  <CollapsibleContent
    data-slot="collapsible-content"
    v-bind="delegatedProps"
    :class="cn('overflow-hidden data-[state=closed]:animate-collapsible-up data-[state=open]:animate-collapsible-down', props.class)"
  >
    <slot />
  </CollapsibleContent>
</template>
