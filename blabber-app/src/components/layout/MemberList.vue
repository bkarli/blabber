<script setup lang="ts">
import { computed } from 'vue';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Avatar, AvatarFallback } from '@/components/ui/avatar';
import { useAppStore } from '@/stores/app';

const props = defineProps<{ spaceId: string }>();
const store = useAppStore();

const members = computed(() =>
  [...store.membersFor(props.spaceId)].sort((a, b) =>
    a.display_name.localeCompare(b.display_name)
  )
);

function initials(name: string) {
  return name.slice(0, 2).toUpperCase();
}

function isYou(authorId: string) {
  return authorId === store.myAuthorId;
}
</script>

<template>
  <div class="flex h-full w-60 flex-col border-l border-border bg-sidebar">
    <div class="flex h-12 shrink-0 items-center border-b border-border px-4">
      <span class="text-xs font-semibold tracking-wide uppercase text-muted-foreground">
        {{ members.length }} Members
      </span>
    </div>

    <ScrollArea class="flex-1 px-2 py-3">
      <div class="flex flex-col gap-0.5">
        <div
          v-for="member in members"
          :key="member.author_id"
          class="flex items-center gap-2 rounded-md px-2 py-1.5 transition-colors hover:bg-accent"
        >
          <Avatar class="h-8 w-8 shrink-0">
            <AvatarFallback class="text-xs">{{ initials(member.display_name) }}</AvatarFallback>
          </Avatar>
          <span class="truncate text-sm">
            {{ member.display_name }}
            <span v-if="isYou(member.author_id)" class="text-muted-foreground">(you)</span>
          </span>
        </div>

        <p v-if="members.length === 0" class="px-2 py-1.5 text-sm text-muted-foreground">
          No members yet.
        </p>
      </div>
    </ScrollArea>
  </div>
</template>
