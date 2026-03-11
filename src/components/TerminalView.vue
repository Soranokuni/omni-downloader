<script setup lang="ts">
import { ref, onMounted, nextTick, onUnmounted } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

interface LogPayload {
  message: string
}

const terminalOutput = ref<string[]>([
  '[system] Initializing Omni Downloader Backend ...',
  '[system] MCP stdio transport bound and GUI listening.',
])

const terminalScroll = ref<HTMLElement | null>(null)
let unlistenLogs: UnlistenFn | null = null

onMounted(async () => {
  try {
    unlistenLogs = await listen<LogPayload>('backend-log', (event) => {
      terminalOutput.value.push(event.payload.message)
      
      // Keep only last 200 lines to preserve DOM performance
      if (terminalOutput.value.length > 200) {
        terminalOutput.value.shift()
      }

      nextTick(() => {
        if (terminalScroll.value) {
          terminalScroll.value.scrollTop = terminalScroll.value.scrollHeight
        }
      })
    })
  } catch (e) {
    console.error("Failed to bind log listener:", e)
  }
})

onUnmounted(() => {
  if (unlistenLogs) unlistenLogs()
})

</script>

<template>
  <div class="h-full w-full relative group">
    <div class="absolute right-0 top-0 p-1 opacity-0 group-hover:opacity-100 transition-opacity z-10">
      <button @click="terminalOutput = []" class="bg-neutral-800 hover:bg-neutral-700 text-neutral-400 p-1.5 rounded transition-colors" title="Clear Terminal">
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"></path><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"></path><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"></path></svg>
      </button>
    </div>
    
    <div ref="terminalScroll" class="h-full overflow-y-auto pr-2 pb-2">
      <div v-for="(line, index) in terminalOutput" :key="index" class="mb-1 leading-relaxed">
        <span class="text-neutral-500">[{{ new Date().toISOString().split('T')[1].slice(0, 8) }}]</span> 
        <span :class="{
          'text-emerald-400': line.includes('[system]'),
          'text-purple-400': line.includes('[ffmpeg]'),
          'text-blue-400': line.includes('[yt-dlp]'),
          'text-yellow-400': line.includes('[scraper]'),
          'text-indigo-400': line.includes('[mcp')
        }" class="ml-2 whitespace-pre-wrap break-words">
          {{ line }}
        </span>
      </div>
    </div>
  </div>
</template>
