<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

interface ProgressPayload {
  id: string
  status: 'pending' | 'downloading' | 'transcoding' | 'done' | 'error'
  progress: number
}

interface QueueItem {
  id: string
  status: 'pending' | 'downloading' | 'transcoding' | 'done' | 'error'
  progress: number
}

const queue = ref<QueueItem[]>([])
let unlistenProgress: UnlistenFn | null = null

onMounted(async () => {
  try {
    unlistenProgress = await listen<ProgressPayload>('download-progress', (event) => {
      const payload = event.payload
      
      const existingTask = queue.value.find(q => q.id === payload.id)
      
      if (existingTask) {
        existingTask.status = payload.status
        existingTask.progress = payload.progress
      } else {
        queue.value.push({
          id: payload.id,
          status: payload.status,
          progress: payload.progress
        })
      }
    })
  } catch (e) {
    console.error("Failed to bind progress listener:", e)
  }
})

onUnmounted(() => {
  if (unlistenProgress) unlistenProgress()
})
</script>

<template>
  <div class="h-full flex flex-col gap-3 pr-2 pb-2 overflow-y-auto">
    <div v-for="item in queue" :key="item.id" 
         class="bg-neutral-900/80 border border-neutral-800/80 p-4 rounded-xl shadow-sm hover:border-neutral-700 transition-colors group">
      
      <div class="flex items-center justify-between mb-2">
        <h3 class="font-medium font-mono text-xs text-neutral-400 truncate pr-4">
          Task ID: {{ item.id }}
        </h3>
        <span class="text-[10px] font-bold uppercase tracking-wider px-2 py-0.5 rounded-full"
              :class="{
                'bg-yellow-500/10 text-yellow-500 border border-yellow-500/20': item.status === 'pending',
                'bg-blue-500/10 text-blue-500 border border-blue-500/20': item.status === 'downloading',
                'bg-purple-500/10 text-purple-500 border border-purple-500/20': item.status === 'transcoding',
                'bg-emerald-500/10 text-emerald-500 border border-emerald-500/20': item.status === 'done',
                'bg-red-500/10 text-red-500 border border-red-500/20': item.status === 'error',
              }">
          {{ item.status }}
        </span>
      </div>

      <div class="flex items-end justify-between mt-4">
        <div class="flex-1 mr-4">
          <div class="h-1.5 w-full bg-neutral-800 rounded-full overflow-hidden relative">
             <div class="absolute inset-y-0 left-0 rounded-full transition-all duration-500 ease-out"
                  :style="{ width: `${item.progress}%` }"
                  :class="{
                    'bg-neutral-600': item.status === 'pending',
                    'bg-blue-500': item.status === 'downloading',
                    'bg-gradient-to-r from-purple-500 to-indigo-500': item.status === 'transcoding',
                    'bg-emerald-500': item.status === 'done',
                    'bg-red-500': item.status === 'error',
                  }">
             </div>
          </div>
        </div>
        <div class="text-xs font-mono text-neutral-500 whitespace-nowrap">
          <span class="font-semibold" :class="item.status === 'done' ? 'text-emerald-500' : 'text-neutral-300'">
            {{ item.progress.toFixed(0) }}%
          </span>
        </div>
      </div>
    </div>

    <!-- Empty State -->
    <div v-if="queue.length === 0" class="h-full flex flex-col items-center justify-center text-neutral-500">
      <svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="mb-3 opacity-50"><path d="M22 12h-4l-3 9L9 3l-3 9H2"></path></svg>
       <p class="text-sm">Queue is empty</p>
    </div>
  </div>
</template>
