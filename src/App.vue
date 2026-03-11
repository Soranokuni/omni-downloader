<script setup lang="ts">
import InputArea from './components/InputArea.vue'
import QueueManager from './components/QueueManager.vue'
import TerminalView from './components/TerminalView.vue'
import SettingsDialog from './components/SettingsDialog.vue'
</script>

<template>
  <div class="h-screen w-screen bg-black text-white p-6 overflow-hidden flex flex-col gap-6">
    <header class="flex items-center justify-between border-b border-neutral-800 pb-4">
      <div class="flex items-center gap-3">
        <div class="w-8 h-8 rounded-lg bg-gradient-to-br from-indigo-500 to-purple-600 flex items-center justify-center font-bold text-white shadow-lg shadow-purple-500/20">
          O
        </div>
        <div>
          <h1 class="text-xl font-bold tracking-tight text-white leading-none">Omni Downloader</h1>
          <p class="text-xs text-neutral-400 mt-1">Autonomous Media Orchestration</p>
        </div>
      </div>
      <div class="flex items-center gap-2">
        <div class="px-2 py-1 rounded text-xs font-medium bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 flex items-center gap-1.5">
          <span class="w-1.5 h-1.5 rounded-full bg-emerald-500 animate-pulse"></span>
          MCP Ready
        </div>
        <SettingsDialog />
      </div>
    </header>

    <main class="flex-1 flex gap-6 min-h-0">
      
      <!-- Left Column (Input + Terminal) -->
      <div class="w-[35%] flex flex-col gap-6 min-h-0">
        
        <!-- Input Area & Controls -->
        <section class="shrink-0 bg-neutral-900/50 border border-neutral-800 rounded-2xl p-5 flex flex-col shadow-xl backdrop-blur-sm relative overflow-hidden group hover:border-neutral-700 transition-colors">
          <div class="absolute inset-0 bg-gradient-to-br from-indigo-500/5 to-transparent opacity-0 group-hover:opacity-100 transition-opacity"></div>
          <h2 class="text-sm font-semibold text-neutral-300 mb-4 flex items-center gap-2 relative z-10 shrink-0">
            <span class="w-1 h-4 rounded-full bg-indigo-500"></span>
            Source Configuration
          </h2>
          <InputArea class="relative z-10" />
        </section>

        <!-- Terminal Output -->
        <section class="flex-1 min-h-0 bg-neutral-950 border border-neutral-800 rounded-2xl flex flex-col shadow-inner relative overflow-hidden">
          <div class="p-3 border-b border-neutral-800/80 flex items-center justify-between bg-black/40 shrink-0">
            <h2 class="text-xs font-mono text-neutral-400 flex items-center gap-2">
              <span class="text-emerald-500">>_</span> stdout
            </h2>
            <div class="flex gap-1.5">
              <div class="w-2.5 h-2.5 rounded-full bg-red-500/80"></div>
              <div class="w-2.5 h-2.5 rounded-full bg-yellow-500/80"></div>
              <div class="w-2.5 h-2.5 rounded-full bg-green-500/80"></div>
            </div>
          </div>
          <TerminalView class="flex-1 min-h-0 p-4 font-mono text-xs" />
        </section>

      </div>

      <!-- Right Column (Queue) -->
      <div class="w-[65%] flex flex-col min-h-0">
        <!-- Queue Manager -->
        <section class="flex-1 min-h-0 bg-neutral-900/50 border border-neutral-800 rounded-2xl flex flex-col shadow-xl backdrop-blur-sm relative overflow-hidden group hover:border-neutral-700 transition-colors">
          <div class="absolute inset-0 bg-gradient-to-bl from-purple-500/5 to-transparent opacity-0 group-hover:opacity-100 transition-opacity"></div>
          <div class="p-5 border-b border-neutral-800/50 relative z-10 flex justify-between items-center bg-neutral-900/40 shrink-0">
             <h2 class="text-sm font-semibold text-neutral-300 flex items-center gap-2">
              <span class="w-1 h-4 rounded-full bg-purple-500"></span>
              Active Telemetry
            </h2>
            <span class="text-xs text-neutral-500 bg-neutral-800/80 px-2 py-0.5 rounded-full">Live Monitor</span>
          </div>
          <QueueManager class="flex-1 min-h-0 relative z-10 p-5" />
        </section>
      </div>

    </main>
  </div>
</template>

<style>
/* Smooth scrollbar for the internal panels */
::-webkit-scrollbar {
  width: 6px;
  height: 6px;
}
::-webkit-scrollbar-track {
  background: transparent;
}
::-webkit-scrollbar-thumb {
  background: #333;
  border-radius: 4px;
}
::-webkit-scrollbar-thumb:hover {
  background: #555;
}
</style>
