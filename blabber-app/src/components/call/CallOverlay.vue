<script setup lang="ts">
import { computed } from 'vue';
import { useAppStore } from '@/stores/app';
import CallFullscreen from '@/components/call/CallFullscreen.vue';
import CallMinimizedPill from '@/components/call/CallMinimizedPill.vue';

const store = useAppStore();
const spaceId = computed(() => store.activeCallSpaceId);
</script>

<template>
  <template v-if="store.activeCallRoomId && spaceId">
    <CallFullscreen v-if="store.callViewMode === 'fullscreen'" :space-id="spaceId" />
    <CallMinimizedPill v-else :space-id="spaceId" />
  </template>
</template>
