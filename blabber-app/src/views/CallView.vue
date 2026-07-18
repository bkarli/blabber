<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { Button } from '@/components/ui/button';
import { Avatar, AvatarFallback } from '@/components/ui/avatar';
import { PhoneOff, Mic } from 'lucide-vue-next';
import { useAppStore } from '@/stores/app';

const route = useRoute();
const router = useRouter();
const store = useAppStore();

const spaceId = route.params.spaceId as string;
const roomId = route.params.roomId as string;

const participants = ref<string[]>([]);
const joining = ref(true);
const error = ref<string | null>(null);

let pollHandle: ReturnType<typeof setInterval> | undefined;

async function refreshParticipants() {
  try {
    participants.value = await store.loadCallParticipants(roomId);
  } catch (e) {
    console.error('failed to load participants', e);
  }
}

async function join() {
  try {
    await store.joinCallRoom(roomId);
    await refreshParticipants();
  } catch (e) {
    error.value = String(e);
  } finally {
    joining.value = false;
  }
}

async function hangUp() {
  try {
    await store.leaveCallRoom();
  } catch (e) {
    console.error('failed to leave call room', e);
  } finally {
    router.push({ name: 'space', params: { spaceId } });
  }
}

onMounted(() => {
  join();
  // simple polling until a live participant-join/leave event exists
  pollHandle = setInterval(refreshParticipants, 3000);
});

onUnmounted(() => {
  if (pollHandle) clearInterval(pollHandle);
});

function initials(id: string) {
  return id.slice(0, 2).toUpperCase();
}
</script>

<template>
  <div class="flex h-full flex-1 flex-col items-center justify-between bg-background p-8">
    <div class="flex flex-1 flex-col items-center justify-center gap-6">
      <p v-if="joining" class="text-sm text-muted-foreground">Joining call...</p>
      <p v-else-if="error" class="text-sm text-destructive">{{ error }}</p>

      <div v-else class="grid grid-cols-3 gap-6">
        <div
          v-for="participant in participants"
          :key="participant"
          class="flex flex-col items-center gap-2"
        >
          <Avatar class="h-20 w-20">
            <AvatarFallback class="text-lg">{{ initials(participant) }}</AvatarFallback>
          </Avatar>
          <span class="max-w-[100px] truncate text-xs text-muted-foreground">
            {{ participant.slice(0, 8) }}
          </span>
        </div>

        <p v-if="participants.length === 0" class="col-span-3 text-sm text-muted-foreground">
          You're the only one here.
        </p>
      </div>
    </div>

    <div class="flex items-center gap-4 pb-8">
      <Button variant="secondary" size="icon" class="h-12 w-12 rounded-full">
        <Mic class="h-5 w-5" />
      </Button>
      <Button variant="destructive" size="icon" class="h-14 w-14 rounded-full" @click="hangUp">
        <PhoneOff class="h-5 w-5" />
      </Button>
    </div>
  </div>
</template>
