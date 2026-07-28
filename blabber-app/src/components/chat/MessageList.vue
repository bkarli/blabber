<script setup lang="ts">
import { computed, watch, onMounted, nextTick } from 'vue';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Avatar, AvatarFallback } from '@/components/ui/avatar';
import { Bubble, BubbleContent, BubbleGroup } from '@/components/ui/bubble';
import { Message, MessageAvatar, MessageContent } from '@/components/ui/message';
import { useAppStore, type MessageContent as MsgContent } from '@/stores/app';

const props = defineProps<{ spaceId: string; roomId: string }>();
const store = useAppStore();

const messages = computed(() => store.messagesFor(props.roomId));

interface MessageGroup {
  author: string;
  sentAt: number;
  contents: MsgContent[];
  isOwn: boolean;
}

const grouped = computed(() => {
  const groups: MessageGroup[] = [];
  for (const msg of messages.value) {
    const isOwn = msg.author === store.myAuthorId;
    const last = groups[groups.length - 1];
    if (last && last.author === msg.author) {
      last.contents.push(msg.content);
    } else {
      groups.push({ author: msg.author, sentAt: msg.sent_at, contents: [msg.content], isOwn });
    }
  }
  return groups;
});

function displayName(authorId: string) {
  return store.displayNameFor(props.spaceId, authorId);
}

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
watch(messages, scrollToBottom, { deep: true});

function formatTime(ms: number) {
  return new Date(ms).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function initials(name: string) {
  return name.slice(0, 2).toUpperCase();
}
</script>

<template>
  <ScrollArea data-message-scroll class="min-h-0 flex-1 px-4">
    <div class="flex flex-col gap-4 py-4">
      <Message
        v-for="(group, i) in grouped"
        :key="`${group.author}-${group.sentAt}-${i}`"
        :align="group.isOwn ? 'end' : 'start'"
        class="max-w-full"
      >
        <MessageAvatar>
          <Avatar>
            <AvatarFallback>{{ initials(displayName(group.author)) }}</AvatarFallback>
          </Avatar>
        </MessageAvatar>
        <MessageContent class="flex-1 min-w-0">
          <div class="mb-1 flex items-baseline gap-2">
            <span class="text-sm font-semibold">{{ displayName(group.author) }}</span>
            <span class="text-xs text-muted-foreground">{{ formatTime(group.sentAt) }}</span>
          </div>

          <BubbleGroup>
            <template v-for="(content, j) in group.contents" :key="j">
              <Bubble
                v-if="content.kind === 'Text'"
                :variant="group.isOwn ? 'default' : 'muted'"
                class="max-w-[320px] w-fit break-words whitespace-pre-wrap"
              >
                <BubbleContent>{{ content.text }}</BubbleContent>
              </Bubble>

              <div v-else class="max-w-[280px] overflow-hidden rounded-lg border border-border">
                <img
                  :src="`data:${content.mime};base64,${content.data_base64}`"
                  :alt="content.filename"
                  class="block max-h-[320px] w-full object-cover"
                  loading="lazy"
                />
              </div>
            </template>
          </BubbleGroup>
        </MessageContent>
      </Message>

      <p v-if="messages.length === 0" class="py-8 text-center text-sm text-muted-foreground">
        No messages yet. Say hello!
      </p>
    </div>
  </ScrollArea>
</template>
