<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { Settings, X } from 'lucide-vue-next'

const isOpen = ref(false)
const config = ref<any>(null)
const isLoading = ref(true)

const loadConfig = async () => {
  try {
    isLoading.value = true
    config.value = await invoke('get_config')
  } catch (e) {
    console.error("Failed to load config:", e)
  } finally {
    isLoading.value = false
  }
}

const saveConfig = async () => {
  try {
    await invoke('save_config', { newConfig: config.value })
    isOpen.value = false
  } catch (e) {
    console.error("Failed to save config:", e)
  }
}

onMounted(() => {
  loadConfig()
})
</script>

<template>
  <div>
    <button @click="isOpen = true" class="p-2 text-neutral-400 hover:text-white transition-colors bg-neutral-800 hover:bg-neutral-700 rounded-lg">
      <Settings :size="20" />
    </button>

    <div v-if="isOpen" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div class="bg-neutral-900 border border-neutral-800 rounded-xl w-full max-w-2xl shadow-2xl overflow-hidden flex flex-col max-h-[90vh]">
        <div class="p-4 border-b border-neutral-800 flex justify-between items-center bg-neutral-900/50">
          <h2 class="text-lg font-semibold text-white">Application Settings</h2>
          <button @click="isOpen = false" class="text-neutral-400 hover:text-white">
            <X :size="20" />
          </button>
        </div>
        
        <div class="p-6 overflow-y-auto flex-1 text-sm text-neutral-300 space-y-6" v-if="!isLoading && config">
          
          <div class="space-y-4">
            <h3 class="font-medium text-white border-b border-neutral-800 pb-1">General Paths</h3>
            
            <div class="grid gap-2">
              <label class="text-xs text-neutral-400 font-medium">NAS Retention Directory</label>
              <input v-model="config.nas_retention_path" type="text" class="w-full bg-neutral-950 border border-neutral-800 rounded-lg px-3 py-2 text-white focus:outline-none focus:border-purple-500 focus:ring-1 focus:ring-purple-500" />
              <span class="text-[10px] text-neutral-500">The directory where original media files are kept for 14 days before auto-cleanup.</span>
            </div>

            <div class="grid gap-2">
              <label class="text-xs text-neutral-400 font-medium">Binaries Directory (yt-dlp, ffmpeg)</label>
              <input v-model="config.binaries_path" type="text" class="w-full bg-neutral-950 border border-neutral-800 rounded-lg px-3 py-2 text-white focus:outline-none focus:border-purple-500 focus:ring-1 focus:ring-purple-500" />
            </div>

            <div class="grid gap-2">
              <label class="text-xs text-neutral-400 font-medium">Export Directory</label>
              <input v-model="config.output_path" type="text" class="w-full bg-neutral-950 border border-neutral-800 rounded-lg px-3 py-2 text-white focus:outline-none focus:border-purple-500 focus:ring-1 focus:ring-purple-500" />
              <span class="text-[10px] text-neutral-500">Transcoded outputs are written here. Original source files still go to retention storage.</span>
            </div>
          </div>

          <div class="space-y-4">
            <div class="flex items-center justify-between border-b border-neutral-800 pb-1">
              <h3 class="font-medium text-white">Encoding Profiles</h3>
              <div class="text-xs text-purple-400">Default: {{ config.default_profile }}</div>
            </div>
            
            <div v-for="(profile, index) in config.profiles" :key="index" class="bg-neutral-950 border border-neutral-800 p-4 rounded-lg space-y-3">
              <div class="flex gap-4">
                <div class="flex-1 grid gap-1">
                  <label class="text-xs text-neutral-500">Profile Name</label>
                  <input v-model="profile.name" type="text" class="bg-neutral-900 border border-neutral-800 rounded px-2 py-1 text-white w-full" />
                </div>
                <div class="w-24 grid gap-1">
                  <label class="text-xs text-neutral-500">Extension</label>
                  <input v-model="profile.extension" type="text" class="bg-neutral-900 border border-neutral-800 rounded px-2 py-1 text-white w-full" />
                </div>
                <div class="flex items-end">
                   <button @click="config.default_profile = profile.name" class="px-3 py-1.5 rounded text-xs" :class="config.default_profile === profile.name ? 'bg-purple-600 text-white' : 'bg-neutral-800 text-neutral-400 hover:bg-neutral-700'">
                     Set Default
                   </button>
                </div>
              </div>
              <div class="grid gap-1">
                 <label class="text-xs text-neutral-500">FFmpeg Arguments (JSON Array format)</label>
                 <textarea :value="JSON.stringify(profile.ffmpeg_args)" @change="(e) => profile.ffmpeg_args = JSON.parse((e.target as HTMLTextAreaElement).value)" class="bg-neutral-900 border border-neutral-800 rounded px-2 py-2 text-neutral-400 text-xs font-mono w-full h-20 resize-y"></textarea>
              </div>
            </div>
          </div>

        </div>
        
        <div class="p-4 border-t border-neutral-800 bg-neutral-900/50 flex justify-end gap-3">
          <button @click="isOpen = false" class="px-4 py-2 text-sm text-neutral-300 hover:text-white transition-colors">Cancel</button>
          <button @click="saveConfig" class="px-6 py-2 bg-white text-black text-sm font-medium rounded-lg hover:bg-neutral-200 transition-colors">Save Settings</button>
        </div>
      </div>
    </div>
  </div>
</template>
