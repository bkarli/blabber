<script setup lang="ts">
import { computed } from 'vue';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Avatar, AvatarFallback } from '@/components/ui/avatar';
import { Server } from 'lucide-vue-next';
import { useAppStore } from '@/stores/app';

const props = defineProps<{ spaceId: string }>();
const store = useAppStore();

const members = computed(() =>
  [...store.membersFor(props.spaceId)].sort((a, b) => {
    if (a.is_relay !== b.is_relay) return a.is_relay ? 1 : -1;
    return a.display_name.localeCompare(b.display_name);
  })
);

const humanCount = computed(() => members.value.filter((m) => !m.is_relay).length);
const relayCount = computed(() => members.value.filter((m) => m.is_relay).length);

function initials(name: string) {
  return name.slice(0, 2).toUpperCase();
}

function isYou(authorId: string) {
  return authorId === store.myAuthorIdFor(props.spaceId);
}
</script>

<template>
  <div class="flex h-full w-60 flex-col border-l border-border bg-sidebar">
    <div class="flex h-12 shrink-0 items-center border-b border-border px-4">
      <span class="text-xs font-semibold tracking-wide uppercase text-muted-foreground">
        {{ humanCount }} Members<template v-if="relayCount"> · {{ relayCount }} Relay{{ relayCount > 1 ? 's' : '' }}</template>
      </span>
    </div>

    <ScrollArea class="flex-1 px-2 py-3">
      <div class="flex flex-col gap-0.5">
        <div
          v-for="member in members"
          :key="member.author_id"
          class="flex items-center gap-2 rounded-md px-2 py-1.5 transition-colors hover:bg-accent"
        >
          <Avatar v-if="!member.is_relay" class="h-8 w-8 shrink-0">
            <AvatarFallback class="text-xs">{{ initials(member.display_name) }}</AvatarFallback>
          </Avatar>
          <div
            v-else
            class="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-muted text-muted-foreground"
            title="Blind relay - can seed this space's data but never decrypt it"
          >
            <Server class="h-4 w-4" />
          </div>
          <span class="truncate text-sm">
            {{ member.display_name }}
            <span v-if="isYou(member.author_id)" class="text-muted-foreground">(you)</span>
            <span v-else-if="member.is_relay" class="text-muted-foreground">(relay)</span>
          </span>
        </div>

        <p v-if="members.length === 0" class="px-2 py-1.5 text-sm text-muted-foreground">
          No members yet.
        </p>
      </div>
    </ScrollArea>
  </div>
</template>
