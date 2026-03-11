<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'

const url = ref('')
const targetName = ref('')
const isProcessing = ref(false)

const config = ref<any>(null)
const selectedProfile = ref('')

onMounted(async () => {
  try {
    config.value = await invoke('get_config')
    selectedProfile.value = config.value.default_profile
  } catch (e) {
    console.error("Failed to fetch initial config", e)
  }
})

const startProcess = async () => {
  if (!url.value || !targetName.value || !selectedProfile.value) return
  isProcessing.value = true
  
  try {
    await invoke('start_download', {
      url: url.value,
      targetFilename: targetName.value,
      profileName: selectedProfile.value
    })
    console.log("Download task dispatched to core pipeline")
    
    setTimeout(() => {
      url.value = ''
      targetName.value = ''
      isProcessing.value = false
    }, 500)
    
  } catch (e) {
    console.error("Failed to trigger pipeline completion:", e)
    isProcessing.value = false
  }
}
</script>

<template>
  <div class="flex flex-col h-full justify-between gap-4">
    <div class="space-y-4">
      <div class="space-y-1.5">
        <label class="text-xs font-medium text-neutral-400 ml-1">Media URL</label>
        <div class="relative group/input">
           <div class="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none text-neutral-500">
             <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"></path><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"></path></svg>
           </div>
           <input v-model="url" type="text" placeholder="https://example.com/article" class="w-full bg-neutral-900 border border-neutral-700/50 rounded-xl py-2.5 pl-9 pr-4 text-sm text-neutral-200 placeholder:text-neutral-600 focus:outline-none focus:ring-2 focus:ring-indigo-500/50 focus:border-indigo-500 transition-all shadow-inner group-hover/input:border-neutral-600" />
        </div>
      </div>

      <div class="space-y-1.5">
        <label class="text-xs font-medium text-neutral-400 ml-1">Target Filename Base</label>
        <div class="relative group/input">
           <div class="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none text-neutral-500">
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z"></path><polyline points="14 2 14 8 20 8"></polyline><line x1="16" y1="13" x2="8" y2="13"></line><line x1="16" y1="17" x2="8" y2="17"></line><line x1="10" y1="9" x2="8" y2="9"></line></svg>
          </div>
          <input v-model="targetName" type="text" placeholder="#1 Iran News" class="w-full bg-neutral-900 border border-neutral-700/50 rounded-xl py-2.5 pl-9 pr-4 text-sm text-neutral-200 placeholder:text-neutral-600 focus:outline-none focus:ring-2 focus:ring-purple-500/50 focus:border-purple-500 transition-all shadow-inner group-hover/input:border-neutral-600" />
        </div>
      </div>
      
      <div class="space-y-1.5" v-if="config">
        <label class="text-xs font-medium text-neutral-400 ml-1">Encoding Profile</label>
        <div class="relative group/input">
           <select v-model="selectedProfile" class="w-full bg-neutral-900 border border-neutral-700/50 rounded-xl py-2.5 pl-3 pr-4 text-sm text-neutral-200 appearance-none focus:outline-none focus:ring-2 focus:ring-emerald-500/50 focus:border-emerald-500 transition-all shadow-inner group-hover/input:border-neutral-600 cursor-pointer">
             <option v-for="profile in config.profiles" :key="profile.name" :value="profile.name">{{ profile.name }} ({{ profile.extension }})</option>
           </select>
           <div class="absolute inset-y-0 right-0 pr-3 flex items-center pointer-events-none text-neutral-500">
             <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="6 9 12 15 18 9"></polyline></svg>
           </div>
        </div>
      </div>
    </div>

    <button @click="startProcess" :disabled="isProcessing || !url || !targetName" class="group relative w-full overflow-hidden rounded-xl bg-gradient-to-r from-indigo-500 to-purple-600 py-3 px-4 text-sm font-semibold text-white shadow-lg shadow-indigo-500/25 transition-all hover:shadow-indigo-500/40 active:scale-[0.98] disabled:opacity-50 disabled:cursor-not-allowed disabled:hover:shadow-none">
      <div class="absolute inset-0 bg-white/20 opacity-0 transition-opacity group-hover:opacity-100"></div>
      <div v-if="isProcessing" class="flex items-center justify-center gap-2">
        <svg class="animate-spin -ml-1 mr-3 h-4 w-4 text-white" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        Initializing Core...
      </div>
      <span v-else class="flex items-center justify-center gap-2">
        Execute Pipeline
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="group-hover:translate-x-1 transition-transform"><line x1="5" y1="12" x2="19" y2="12"></line><polyline points="12 5 19 12 12 19"></polyline></svg>
      </span>
    </button>
  </div>
</template>
