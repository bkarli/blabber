<script setup lang="ts">
import { Button } from '@/components/ui/button';
import { Avatar, AvatarFallback } from '@/components/ui/avatar';
import { PhoneOff, Mic, MicOff, Minimize2 } from 'lucide-vue-next';
import { useAppStore } from '@/stores/app';

const props = defineProps<{ spaceId: string }>();
const store = useAppStore();

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
  <div class="fixed inset-0 z-50 flex h-screen w-screen flex-col items-center justify-center gap-10 bg-background p-8">
    <Button
      variant="secondary"
      class="absolute right-4 top-4 h-11 gap-2 rounded-full px-4 text-sm font-medium shadow-md"
      title="Minimize"
      @click="store.minimizeCall()"
    >
      <Minimize2 class="h-4 w-4" />
      Minimize
    </Button>

    <div class="flex w-full max-w-2xl flex-wrap items-start justify-center gap-6">
      <div
        v-for="participant in store.callParticipants"
        :key="participant"
        class="flex flex-col items-center gap-2"
      >
        <Avatar class="h-20 w-20">
          <AvatarFallback class="text-lg">{{ initials(store.displayNameForEndpoint(props.spaceId, participant)) }}</AvatarFallback>
        </Avatar>
        <span class="max-w-[100px] truncate text-xs text-muted-foreground">
          {{ store.displayNameForEndpoint(props.spaceId, participant) }}
        </span>
      </div>

      <p v-if="store.callParticipants.length === 0" class="text-sm text-muted-foreground">
        You're the only one here.
      </p>
    </div>

    <div class="flex items-center gap-4">
      <Button :variant="store.isMuted ? 'destructive' : 'secondary'" size="icon" class="h-12 w-12 rounded-full"
              @click="toggleMute()"
      >
        <MicOff v-if="store.isMuted" class="h-5 w-5" />
        <Mic v-else class="h-5 w-5" />
      </Button>
      <Button variant="destructive" size="icon" class="h-14 w-14 rounded-full" @click="hangUp">
        <PhoneOff class="h-5 w-5" />
      </Button>
    </div>
  </div>
</template>
