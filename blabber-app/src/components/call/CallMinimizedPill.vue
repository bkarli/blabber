<script setup lang="ts">
import { ref } from 'vue';
import { useDraggable } from '@vueuse/core';
import { Button } from '@/components/ui/button';
import { Avatar, AvatarFallback } from '@/components/ui/avatar';
import { PhoneOff, Mic, MicOff, GripVertical, Maximize2 } from 'lucide-vue-next';
import { useAppStore } from '@/stores/app';

const MAX_VISIBLE_AVATARS = 3;

const props = defineProps<{ spaceId: string }>();
const store = useAppStore();

const pillEl = ref<HTMLElement | null>(null);
const dragHandleEl = ref<HTMLElement | null>(null);

const { style } = useDraggable(pillEl, {
  initialValue: { x: 24, y: Math.max(24, window.innerHeight - 160) },
  handle: dragHandleEl,
  containerElement: () => document.body,
  restrictInView: true,
});

async function hangUp() {
  try {
    await store.leaveCallRoom();
  } catch (e) {
    console.error('failed to leave call room', e);
  }
}

async function toggleMute() {
  try {
    await store.setMuted(!store.isMuted);
  } catch (e) {
    console.error('failed to toggle mute', e);
  }
}

function initials(name: string) {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return '??';
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return (parts[0][0] + parts[1][0]).toUpperCase();
}
</script>

<template>
  <div
    ref="pillEl"
    class="fixed z-50 flex select-none items-center gap-4 rounded-2xl border border-border bg-card px-6 py-4 shadow-xl"
    :style="style"
  >
    <div
      ref="dragHandleEl"
      class="flex cursor-grab items-center gap-3 active:cursor-grabbing"
      title="Drag to move"
    >
      <GripVertical class="h-4 w-4 shrink-0 text-muted-foreground/60" />

      <div class="flex -space-x-4">
        <Avatar
          v-for="participant in store.callParticipants.slice(0, MAX_VISIBLE_AVATARS)"
          :key="participant"
          class="h-14 w-14 border-2 border-card"
        >
          <AvatarFallback class="text-sm">
            {{ initials(store.displayNameForEndpoint(props.spaceId, participant)) }}
          </AvatarFallback>
        </Avatar>
      </div>
      <span v-if="store.callParticipants.length > MAX_VISIBLE_AVATARS" class="text-sm text-muted-foreground">
        +{{ store.callParticipants.length - MAX_VISIBLE_AVATARS }}
      </span>
      <span v-if="store.callParticipants.length === 0" class="text-sm text-muted-foreground">
        Call
      </span>
    </div>

    <div class="flex items-center gap-2">
      <Button
        variant="ghost"
        size="icon"
        class="h-11 w-11 rounded-full text-muted-foreground"
        title="Expand"
        @click.stop="store.expandCall()"
      >
        <Maximize2 class="h-5 w-5" />
      </Button>
      <Button
        variant="ghost"
        size="icon"
        class="h-11 w-11 rounded-full"
        :class="store.isMuted ? 'text-destructive' : 'text-muted-foreground'"
        @click.stop="toggleMute"
      >
        <MicOff v-if="store.isMuted" class="h-5 w-5" />
        <Mic v-else class="h-5 w-5" />
      </Button>
      <Button
        variant="ghost"
        size="icon"
        class="h-11 w-11 rounded-full text-destructive hover:bg-destructive/10"
        @click.stop="hangUp"
      >
        <PhoneOff class="h-5 w-5" />
      </Button>
    </div>
  </div>
</template>
