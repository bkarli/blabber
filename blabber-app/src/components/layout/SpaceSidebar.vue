<script setup lang="ts">
import { ref, computed } from 'vue';
import { useRouter, useRoute } from 'vue-router';
import { Avatar, AvatarFallback } from '@/components/ui/avatar';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Plus } from 'lucide-vue-next';
import { useAppStore } from '@/stores/app';
import CreateOrJoinSpaceDialog from '@/components/space/CreateOrJoinSpaceDialog.vue';

const router = useRouter();
const route = useRoute();
const store = useAppStore();

const dialogOpen = ref(false);

const activeSpaceId = computed(() => route.params.spaceId as string | undefined);

const palette = ['bg-chart-1', 'bg-chart-2', 'bg-chart-3', 'bg-chart-4', 'bg-chart-5'];

function colorFor(id: string) {
  let hash = 0;
  for (let i = 0; i < id.length; i++) {
    hash = (hash * 31 + id.charCodeAt(i)) >>> 0;
  }
  return palette[hash % palette.length];
}

function initialsFor(name: string) {
  return name
    .split(/\s+/)
    .slice(0, 2)
    .map((word) => word[0]?.toUpperCase())
    .join('');
}

function openSpace(spaceId: string) {
  router.push({ name: 'space', params: { spaceId } });
}
</script>

<template>
  <TooltipProvider :delay-duration="200">
    <div class="flex h-full w-[72px] flex-col border-r border-border bg-sidebar">
      <!-- Space list -->
      <ScrollArea class="flex-1 w-full">
        <div class="flex flex-col items-center gap-2 py-3">
          <Tooltip v-for="space in store.spaces" :key="space.id">
            <TooltipTrigger as-child>
              <button
                class="group relative flex h-12 w-12 items-center justify-center rounded-2xl transition-all hover:rounded-xl"
                @click="openSpace(space.id)"
              >
                <span
                  class="absolute -left-3 w-1 rounded-r-full bg-foreground transition-all"
                  :class="[
                    activeSpaceId === space.id
                      ? 'h-8 opacity-100'
                      : 'h-2 opacity-0 group-hover:h-5 group-hover:opacity-60',
                  ]"
                />
                <Avatar class="h-12 w-12 rounded-2xl transition-all group-hover:rounded-xl">
                  <AvatarFallback :class="[colorFor(space.id), 'rounded-2xl text-sm font-semibold text-white transition-all group-hover:rounded-xl']">
                    {{ initialsFor(space.name) }}
                  </AvatarFallback>
                </Avatar>
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">{{ space.name }}</TooltipContent>
          </Tooltip>
        </div>
      </ScrollArea>

      <!-- Add-space button -->
      <div class="flex shrink-0 items-center justify-center py-3">
        <Tooltip>
          <TooltipTrigger as-child>
            <button
              class="flex h-12 w-12 items-center justify-center rounded-2xl bg-secondary text-primary transition-all hover:rounded-xl hover:bg-primary hover:text-primary-foreground"
              @click="dialogOpen = true"
            >
              <Plus class="h-6 w-6" />
            </button>
          </TooltipTrigger>
          <TooltipContent side="right">Add a space</TooltipContent>
        </Tooltip>
      </div>
    </div>
  </TooltipProvider>

  <CreateOrJoinSpaceDialog v-model:open="dialogOpen" />
</template>
