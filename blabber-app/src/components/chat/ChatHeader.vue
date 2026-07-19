<script setup lang="ts">
import { computed } from 'vue';
import { Hash, Users } from 'lucide-vue-next';
import { Button } from '@/components/ui/button';
import { useAppStore } from '@/stores/app';

const props = defineProps<{ spaceId: string; roomId: string }>();
const emit = defineEmits<{ 'toggle-members': [] }>();

const store = useAppStore();

const room = computed(() =>
  store.roomsFor(props.spaceId).find((r) => r.id === props.roomId)
);
</script>

<template>
  <div class="flex h-12 shrink-0 items-center justify-between border-b border-border px-4">
    <div class="flex items-center gap-2 min-w-0">
      <Hash class="h-4 w-4 shrink-0 text-muted-foreground" />
      <span class="truncate font-semibold">{{ room?.name ?? 'Room' }}</span>
    </div>
    <Button
      variant="ghost"
      size="icon"
      class="h-8 w-8 text-muted-foreground hover:text-foreground"
      @click="emit('toggle-members')"
    >
      <Users class="h-4 w-4" />
    </Button>
  </div>
</template>
