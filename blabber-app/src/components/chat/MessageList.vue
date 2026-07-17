<script setup lang="ts">
import { computed, watch, onMounted, nextTick } from 'vue';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Avatar, AvatarFallback } from '@/components/ui/avatar';
import { Bubble, BubbleContent, BubbleGroup } from '@/components/ui/bubble';
import { Message, MessageAvatar, MessageContent, MessageFooter } from '@/components/ui/message';
import { useAppStore } from '@/stores/app';

const props = defineProps<{ spaceId: string; roomId: string }>();
const store = useAppStore();

const messages = computed(() => store.messagesFor(props.roomId));

// group consecutive messages from the same author so they render as one
// BubbleGroup instead of a separate avatar/row per message
const grouped = computed(() => {
  const groups: { author: string; sentAt: number; contents: string[] }[] = [];
  for (const msg of messages.value) {
    const last = groups[groups.length - 1];
    if (last && last.author === msg.author) {
      last.contents.push(msg.content);
    } else {
      groups.push({ author: msg.author, sentAt: msg.sent_at, contents: [msg.content] });
    }
  }
  return groups;
});

function scrollToBottom() {
  nextTick(() => {
    const viewport = document.querySelector('[data-message-scroll] [data-reka-scroll-area-viewport]');
    viewport?.scrollTo({ top: viewport.scrollHeight, behavior: 'smooth' });
  });
}

async function load() {
  await store.loadMessages(props.spaceId, props.roomId);
  scrollToBottom();
}

onMounted(load);
watch(() => props.roomId, load);
watch(messages, scrollToBottom, { deep: false });

function formatTime(ms: number) {
  return new Date(ms).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function initials(authorId: string) {
  return authorId.slice(0, 2).toUpperCase();
}
</script>

<template>
  <ScrollArea data-message-scroll class="flex-1 px-4">
    <div class="flex flex-col gap-4 py-4">
      <Message v-for="(group, i) in grouped" :key="`${group.author}-${group.sentAt}-${i}`">
        <MessageAvatar>
          <Avatar>
            <AvatarFallback>{{ initials(group.author) }}</AvatarFallback>
          </Avatar>
        </MessageAvatar>
        <MessageContent>
          <div class="mb-1 flex items-baseline gap-2">
            <span class="text-sm font-semibold">{{ group.author.slice(0, 8) }}</span>
            <span class="text-xs text-muted-foreground">{{ formatTime(group.sentAt) }}</span>
          </div>
          <BubbleGroup>
            <Bubble v-for="(content, j) in group.contents" :key="j" variant="muted">
              <BubbleContent>{{ content }}</BubbleContent>
            </Bubble>
          </BubbleGroup>
        </MessageContent>
      </Message>

      <p v-if="messages.length === 0" class="py-8 text-center text-sm text-muted-foreground">
        No messages yet. Say hello!
      </p>
    </div>
  </ScrollArea>
</template>
