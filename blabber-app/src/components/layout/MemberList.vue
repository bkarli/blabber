<script setup lang="ts">
import { computed, onMounted, onUnmounted } from 'vue';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Avatar, AvatarFallback } from '@/components/ui/avatar';
import { Server } from 'lucide-vue-next';
import { useAppStore, type Member } from '@/stores/app';

const CONNECTION_POLL_INTERVAL_MS = 5000;

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

// gossip's NeighborUp/NeighborDown can only report on remote peers, never on
// yourself, so store.isOnline never covers own entry
function isOnline(member: Member) {
  return store.isOnline(props.spaceId, member.endpoint_id) || isYou(member.author_id);
}

// connection type has no push event, poll while this
// list is actually visible, scoped to this component's lifecycle.
let connectionPollHandle: ReturnType<typeof setInterval> | undefined;
onMounted(() => {
  store.loadConnectionTypes(props.spaceId);
  connectionPollHandle = setInterval(() => store.loadConnectionTypes(props.spaceId), CONNECTION_POLL_INTERVAL_MS);
});
onUnmounted(() => clearInterval(connectionPollHandle));

// dot color puts signals together: red means offline, green/orange
// distinguish a direct P2P connection from one going through iroh's relay
function statusColor(member: Member) {
  if (!isOnline(member)) return 'bg-red-500';
  return store.connectionTypeFor(props.spaceId, member.endpoint_id) === 'relayed' ? 'bg-orange-500' : 'bg-green-500';
}

function statusTitle(member: Member) {
  if (!isOnline(member)) return 'Offline';
  return store.connectionTypeFor(props.spaceId, member.endpoint_id) === 'relayed'
    ? 'Online - connected via relay (NAT traversal fallback)'
    : 'Online - direct connection';
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
          <div v-if="!member.is_relay" class="relative shrink-0">
            <Avatar class="h-8 w-8">
              <AvatarFallback class="text-xs">{{ initials(member.display_name) }}</AvatarFallback>
            </Avatar>
            <span
              class="absolute -bottom-0.5 -right-0.5 h-2.5 w-2.5 rounded-full border-2 border-sidebar"
              :class="statusColor(member)"
              :title="statusTitle(member)"
            />
          </div>
          <div
            v-else
            class="relative flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-muted text-muted-foreground"
            title="Blind relay - can seed this space's data but never decrypt it"
          >
            <Server class="h-4 w-4" />
            <span
              class="absolute -bottom-0.5 -right-0.5 h-2.5 w-2.5 rounded-full border-2 border-sidebar"
              :class="statusColor(member)"
              :title="statusTitle(member)"
            />
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
