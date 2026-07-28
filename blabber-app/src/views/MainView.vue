<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { useRoute } from 'vue-router';
import SpaceSidebar from '@/components/layout/SpaceSidebar.vue';
import RoomSidebar from '@/components/layout/RoomSidebar.vue';
import MemberList from '@/components/layout/MemberList.vue';
import ChatHeader from '@/components/chat/ChatHeader.vue';
import MessageList from '@/components/chat/MessageList.vue';
import MessageInput from '@/components/chat/MessageInput.vue';
import { MessageSquare } from 'lucide-vue-next';
import { useAppStore } from '@/stores/app';

const route = useRoute();
const store = useAppStore();
const spaceId = computed(() => route.params.spaceId as string | undefined);
const roomId = computed(() => route.params.roomId as string | undefined);

const membersOpen = ref(true);

watch(roomId, (id) => store.setActiveRoom(id ?? null), { immediate: true });
</script>

<template>
  <div class="flex h-screen w-screen overflow-hidden bg-sidebar text-foreground">
    <SpaceSidebar />
    <RoomSidebar v-if="spaceId" :space-id="spaceId" :active-room-id="roomId" />

    <main class="flex min-h-0 flex-1 flex-col bg-background">
      <template v-if="spaceId && roomId">
        <ChatHeader :space-id="spaceId" :room-id="roomId" @toggle-members="membersOpen = !membersOpen" />
        <MessageList :space-id="spaceId" :room-id="roomId" />
        <MessageInput :space-id="spaceId" :room-id="roomId" />
      </template>

      <template v-else-if="spaceId">
        <div class="flex flex-1 flex-col items-center justify-center gap-3">
          <div class="flex h-16 w-16 items-center justify-center rounded-full bg-muted">
            <MessageSquare class="h-8 w-8 text-muted-foreground" />
          </div>
          <p class="text-sm text-muted-foreground">Select a room to start chatting.</p>
        </div>
      </template>

      <template v-else>
        <div class="flex flex-1 flex-col items-center justify-center gap-3">
          <div class="flex h-16 w-16 items-center justify-center rounded-full bg-muted">
            <MessageSquare class="h-8 w-8 text-muted-foreground" />
          </div>
          <p class="text-sm text-muted-foreground">Select a space to get started, or create one.</p>
        </div>
      </template>
    </main>

    <MemberList v-if="spaceId && roomId && membersOpen" :space-id="spaceId" />
  </div>
</template>
