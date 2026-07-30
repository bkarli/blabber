<script setup lang="ts">
import { computed, watch, onMounted, nextTick, ref } from 'vue';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Avatar, AvatarFallback } from '@/components/ui/avatar';
import { Bubble, BubbleContent, BubbleGroup } from '@/components/ui/bubble';
import { Message, MessageAvatar, MessageContent } from '@/components/ui/message';
import { Dialog, DialogContent } from '@/components/ui/dialog';
import { useAppStore, type MessageContent as MsgContent } from '@/stores/app';
import { File } from 'lucide-vue-next';

const props = defineProps<{ spaceId: string; roomId: string }>();
const store = useAppStore();

const messages = computed(() => store.messagesFor(props.roomId));

interface ContentWithTime {
  content: MsgContent;
  sentAt: number;
}

interface MessageGroup {
  author: string;
  sentAt: number; // still useful for the header timestamp
  contents: ContentWithTime[];
  isOwn: boolean;
}

const grouped = computed(() => {
  const groups: MessageGroup[] = [];
  for (const msg of messages.value) {
    const isOwn = msg.author === store.myAuthorIdFor(props.spaceId);
    const last = groups[groups.length - 1];
    if (last && last.author === msg.author) {
      last.contents.push({ content: msg.content, sentAt: msg.sent_at });
    } else {
      groups.push({
        author: msg.author,
        sentAt: msg.sent_at,
        contents: [{ content: msg.content, sentAt: msg.sent_at }],
        isOwn,
      });
    }
  }
  return groups;
});

const lightboxOpen = ref(false);
const lightboxSrc = ref<string | null>(null);

async function openFullImage(mediaKey: string, mime: string) {
  const base64 = await store.getMedia(props.spaceId, props.roomId, mediaKey);
  if (base64) {
    lightboxSrc.value = `data:${mime};base64,${base64}`;
    lightboxOpen.value = true;
  }
}

import { save } from '@tauri-apps/plugin-dialog';
import { writeFile } from '@tauri-apps/plugin-fs';

async function downloadFile(mediaKey: string, filename: string) {
  try {
    const base64 = await store.getMedia(props.spaceId, props.roomId, mediaKey);
    if (!base64) {
      console.error('file content not available');
      return;
    }

    const savePath = await save({ defaultPath: filename });
    if (!savePath) return; // user cancelled

    const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
    await writeFile(savePath, bytes);
  } catch (e) {
    console.error('failed to download file', e);
  }
}

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
  if (store.messagesFor(props.roomId).length === 0) {
    await store.loadMessages(props.spaceId, props.roomId);
  }
  scrollToBottom();
}

onMounted(load);
watch(() => props.roomId, load);
watch(messages, scrollToBottom, { deep: true });

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
            <template v-for="(item, j) in group.contents" :key="j">
              <Bubble
                v-if="'Text' in item.content"
                :variant="group.isOwn ? 'default' : 'muted'"
                class="max-w-[320px] w-fit break-words whitespace-pre-wrap"
              >
                <BubbleContent>{{ item.content.Text.text }}</BubbleContent>
              </Bubble>

              <div
                v-else-if="'Image' in item.content"
                class="max-w-[240px] cursor-pointer overflow-hidden rounded-lg border border-border"
                @click="openFullImage(item.content.Image.media_key, item.content.Image.mime)"
              >
                <img
                  :src="`data:${item.content.Image.mime};base64,${item.content.Image.thumbnail_base64}`"
                  :alt="item.content.Image.filename"
                  class="block max-h-[240px] max-w-[240px] w-auto h-auto object-contain"
                  loading="lazy"
                />
              </div>

              <div
                v-else-if="'File' in item.content"
                class="flex items-center gap-2 rounded-lg border border-border p-3 cursor-pointer"
                @click="downloadFile(item.content.File.media_key, item.content.File.filename)"
              >
                <File class="h-6 w-6 text-muted-foreground" />
                <div class="min-w-0">
                  <p class="truncate text-sm font-medium">{{ item.content.File.filename }}</p>
                  <p class="text-xs text-muted-foreground">{{ (item.content.File.size / 1024).toFixed(1) }} KB</p>
                </div>
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

  <Dialog v-model:open="lightboxOpen">
    <DialogContent class="max-w-3xl">
      <img v-if="lightboxSrc" :src="lightboxSrc" class="w-full rounded-lg" />
    </DialogContent>
  </Dialog>
</template>
