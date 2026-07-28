<script setup lang="ts">
import { ref, computed, onMounted, watch } from 'vue';
import { useAuthStore } from '@/stores/auth';
import { useRouter } from 'vue-router';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Button } from '@/components/ui/button';
import {
  AlertDialog, AlertDialogContent, AlertDialogHeader, AlertDialogTitle, AlertDialogDescription,
  AlertDialogFooter, AlertDialogCancel, AlertDialogAction,
} from '@/components/ui/alert-dialog';
import { Share2, Check, Plus, Hash, Settings, LogOut } from 'lucide-vue-next';
import { useAppStore } from '@/stores/app';
import CreateRoomDialog from '@/components/room/CreateRoomDialog.vue';
import CreateChannelDialog from '@/components/room/CreateChannelDialog.vue';
import IdentitySettingsDrawer from '@/components/settings/IdentitySettingsDrawer.vue';

// props must be declared BEFORE anything below references it
const props = defineProps<{ spaceId: string; activeRoomId?: string }>();

const router = useRouter();
const store = useAppStore();
const auth = useAuthStore();

const createRoomOpen = ref(false);
const createChannelOpen = ref(false);
const settingsOpen = ref(false);
const confirmLeaveOpen = ref(false);
const leaving = ref(false);
const leaveError = ref<string | null>(null);

const space = computed(() => store.spaces.find((s) => s.id === props.spaceId));
const rooms = computed(() => store.roomsFor(props.spaceId));
const channels = computed(() => store.channelsFor(props.spaceId));

const copied = ref(false);

async function handleShare() {
  try {
    const ticket = await store.getInvite(props.spaceId);
    await navigator.clipboard.writeText(ticket);
    copied.value = true;
    setTimeout(() => { copied.value = false; }, 1500);
  } catch (e) {
    console.error('failed to copy invite', e);
  }
}
function openRoom(roomId: string) {
  router.push({ name: 'room', params: { spaceId: props.spaceId, roomId } });
}

async function handleLeave() {
  leaveError.value = null;
  leaving.value = true;
  try {
    await store.leaveSpace(props.spaceId);
    confirmLeaveOpen.value = false;
    router.push({ name: 'spaces' });
  } catch (e) {
    leaveError.value = String(e);
  } finally {
    leaving.value = false;
  }
}


async function joinChannel(channelId: string) {
  router.push({ name: 'call', params: { spaceId: props.spaceId, roomId: channelId } });
}
// this block must come after `props` is declared above
onMounted(() => {
  store.loadRooms(props.spaceId);
  store.loadChannels(props.spaceId);
  store.loadMembers(props.spaceId);
});

watch(
  () => props.spaceId,
  (newSpaceId) => {
    store.loadRooms(newSpaceId);
    store.loadChannels(newSpaceId);
    store.loadMembers(newSpaceId);
  }
);
</script>

<template>
  <div class="flex h-full w-60 flex-col border-r border-border bg-sidebar">
    <div class="flex h-12 shrink-0 items-center justify-between border-b border-border px-4">
      <span class="truncate font-semibold">{{ space?.name ?? 'Space' }}</span>
      <div class="flex items-center gap-1">
        <Button
          variant="ghost"
          size="icon"
          class="h-7 w-7 text-muted-foreground hover:text-foreground"
          @click="handleShare"
        >
          <Check v-if="copied" class="h-4 w-4 text-primary" />
          <Share2 v-else class="h-4 w-4" />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          class="h-7 w-7 text-muted-foreground hover:text-destructive"
          @click="confirmLeaveOpen = true"
        >
          <LogOut class="h-4 w-4" />
        </Button>
      </div>
    </div>

    <ScrollArea class="flex-1 px-2 py-3">
      <div class="flex items-center justify-between px-2 pb-2">
        <span class="text-xs font-semibold tracking-wide uppercase text-muted-foreground">Rooms</span>
        <button
          class="text-muted-foreground transition-colors hover:text-foreground"
          @click="createRoomOpen = true"
        >
          <Plus class="h-4 w-4" />
        </button>
      </div>

      <div class="flex flex-col gap-0.5">
        <button
          v-for="room in rooms"
          :key="room.id"
          class="flex items-center gap-2 rounded-md px-2 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          :class="{ 'bg-accent text-foreground': room.id === activeRoomId }"
          @click="openRoom(room.id)"
        >
          <Hash class="h-4 w-4 shrink-0" />
          <span class="truncate">{{ room.name }}</span>
          <span
            v-if="store.isRoomUnread(room.id)"
            class="ml-auto h-2 w-2 shrink-0 rounded-full bg-green-500"
          />
        </button>

        <p v-if="rooms.length === 0" class="px-2 py-1.5 text-sm text-muted-foreground">
          No rooms yet.
        </p>
      </div>

      <div class="flex items-center justify-between px-2 pb-2">
        <span class="text-xs font-semibold tracking-wide uppercase text-muted-foreground">Channels</span>
        <button
          class="text-muted-foreground transition-colors hover:text-foreground"
          @click="createChannelOpen = true"
        >
          <Plus class="h-4 w-4" />
        </button>
      </div>

      <div class="flex flex-col gap-0.5">
        <button
          v-for="channel in channels"
          :key="channel.id"
          class="flex items-center gap-2 rounded-md px-2 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          @click="joinChannel(channel.id)"
        >
          <Hash class="h-4 w-4 shrink-0" />
          <span class="truncate">{{ channel.name }}</span>
        </button>

        <p v-if="rooms.length === 0" class="px-2 py-1.5 text-sm text-muted-foreground">
          No channels yet.
        </p>
      </div>

    </ScrollArea>

    <div class="flex h-16 shrink-0 items-center gap-3 border-t border-border px-3">
      <div class="flex h-10 w-10 items-center justify-center rounded-full bg-primary text-sm font-semibold text-primary-foreground">
        Me
      </div>
      <div class="flex-1 min-w-0">
        <p class="truncate text-sm font-medium">{{ auth.displayName ?? 'You' }}</p>
        <p class="truncate text-xs text-muted-foreground">Online</p>
      </div>
      <Button variant="ghost" size="icon" class="h-8 w-8 text-muted-foreground hover:text-foreground" @click="settingsOpen = true">
        <Settings class="h-4 w-4" />
      </Button>
    </div>

    <CreateRoomDialog v-model:open="createRoomOpen" :space-id="spaceId" />
    <CreateChannelDialog v-model:open="createChannelOpen" :space-id="spaceId" />
    <IdentitySettingsDrawer v-model:open="settingsOpen" />

    <AlertDialog v-model:open="confirmLeaveOpen">
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Leave "{{ space?.name ?? 'this space' }}"?</AlertDialogTitle>
          <AlertDialogDescription>
            You will lose access to all rooms and channels unless you are invited back.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <p v-if="leaveError" class="text-sm text-destructive">{{ leaveError }}</p>
        <AlertDialogFooter>
          <AlertDialogCancel :disabled="leaving">Cancel</AlertDialogCancel>
          <AlertDialogAction
            class="bg-destructive text-white hover:bg-destructive/90"
            :disabled="leaving"
            @click.prevent="handleLeave"
          >
            {{ leaving ? 'Leaving...' : 'Leave Space' }}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </div>
</template>
