<script setup lang="ts">
import { computed, ref, onMounted, onUnmounted } from 'vue'

const message = ref('Connecing to Rust...')
type Account = {
  id: string
  name: string
  key: string
  usage_str: string
  status: string
  primary_used: number
  secondary_used: number
  primary_window_present?: boolean
  secondary_window_present?: boolean
  primary_window_minutes?: number | null
  secondary_window_minutes?: number | null
  primary_reset_at?: number | null
  secondary_reset_at?: number | null
}

type CardWindow = {
  key: 'primary' | 'secondary'
  label: string
  used: number
  resetAt?: number | null
}

type DisplayCard = {
  id: string
  source: 'account' | 'aggregate'
  name: string
  status: string
  usageLabel: string
  memberCount: number
  windows: CardWindow[]
  account?: Account
}

const stats = ref<{ tasks: number, accounts: Account[] }>({ tasks: 0, accounts: [] })
let statusInterval: number | null = null

const showAddModal = ref(false)
const showSettingsModal = ref(false)
const dashboardMode = ref<'normal' | 'aggregate'>('normal')

const isSyncing = ref(false)
const editingId = ref<string | null>(null)
const editName = ref('')

const newAccount = ref({ name: '', key: '' })
const settings = ref({ polling_enabled: true, polling_interval_secs: 15, default_quota_view: 'remaining' })

const displayModes = ref<Record<string, boolean>>({})

const toggleMode = (id: string) => {
  displayModes.value[id] = !isRemainingMode(id)
}

const isRemainingMode = (id: string) => displayModes.value[id] ?? (settings.value.default_quota_view !== 'limit')

const displayKey = (key: string) => {
  if (!key) return '***'
  return key.length > 20 ? `${key.slice(0, 10)}...${key.slice(-6)}` : '***'
}

const formatWindowLabel = (windowMinutes: number | null | undefined, fallback: string): string => {
  if (!windowMinutes || windowMinutes <= 0) return fallback
  if (windowMinutes % (60 * 24 * 7) === 0) return 'Weekly'
  if (windowMinutes % (60 * 24) === 0) {
    const days = windowMinutes / (60 * 24)
    return `${days}d`
  }
  if (windowMinutes % 60 === 0) return `${windowMinutes / 60}h`
  return `${windowMinutes}m`
}

const primaryLimitLabel = (acc: Account) => formatWindowLabel(acc.primary_window_minutes, '5h')
const secondaryLimitLabel = (acc: Account) => formatWindowLabel(acc.secondary_window_minutes, 'Weekly')
const hasPrimaryWindow = (acc: Account) => acc.primary_window_present ?? false
const hasSecondaryWindow = (acc: Account) => acc.secondary_window_present ?? false
const normalizePlanLabel = (acc: Account) => (acc.usage_str || 'UNKNOWN').trim().toUpperCase()
const formatAccountCount = (count: number) => `${count} account${count === 1 ? '' : 's'}`

const buildWindowsFromAccount = (acc: Account): CardWindow[] => {
  const windows: CardWindow[] = []

  if (hasPrimaryWindow(acc)) {
    windows.push({
      key: 'primary',
      label: primaryLimitLabel(acc),
      used: acc.primary_used,
      resetAt: acc.primary_reset_at
    })
  }

  if (hasSecondaryWindow(acc)) {
    windows.push({
      key: 'secondary',
      label: secondaryLimitLabel(acc),
      used: acc.secondary_used,
      resetAt: acc.secondary_reset_at
    })
  }

  return windows
}

const buildAccountCard = (acc: Account): DisplayCard => ({
  id: acc.id,
  source: 'account',
  name: acc.name,
  status: acc.status,
  usageLabel: acc.usage_str || '$0.00',
  memberCount: 1,
  windows: buildWindowsFromAccount(acc),
  account: acc
})

const average = (values: number[]) => values.reduce((sum, value) => sum + value, 0) / values.length

const earliestReset = (values: Array<number | null | undefined>) => {
  const valid = values.filter((value): value is number => typeof value === 'number' && value > 0)
  if (valid.length === 0) return null
  return Math.min(...valid)
}

const buildAggregateCard = (planLabel: string, accounts: Account[]): DisplayCard => {
  const primaryAccounts = accounts.filter(hasPrimaryWindow)
  const secondaryAccounts = accounts.filter(hasSecondaryWindow)
  const windows: CardWindow[] = []

  if (primaryAccounts.length > 0) {
    windows.push({
      key: 'primary',
      label: primaryLimitLabel(primaryAccounts[0]),
      used: average(primaryAccounts.map((acc) => acc.primary_used)),
      resetAt: earliestReset(primaryAccounts.map((acc) => acc.primary_reset_at))
    })
  }

  if (secondaryAccounts.length > 0) {
    windows.push({
      key: 'secondary',
      label: secondaryLimitLabel(secondaryAccounts[0]),
      used: average(secondaryAccounts.map((acc) => acc.secondary_used)),
      resetAt: earliestReset(secondaryAccounts.map((acc) => acc.secondary_reset_at))
    })
  }

  return {
    id: `aggregate:${planLabel}`,
    source: 'aggregate',
    name: planLabel,
    status: 'Active',
    usageLabel: planLabel,
    memberCount: accounts.length,
    windows
  }
}

const isAggregatableAccount = (acc: Account) => {
  const usage = (acc.usage_str || '').trim()
  return acc.status === 'Active' && usage.length > 0 && !usage.startsWith('$')
}

const normalCards = computed(() => stats.value.accounts.map(buildAccountCard))

const aggregateCards = computed(() => {
  const groupedAccounts = new Map<string, Account[]>()
  const orderedEntries: Array<DisplayCard | { kind: 'group', groupKey: string }> = []

  for (const acc of stats.value.accounts) {
    if (!isAggregatableAccount(acc)) {
      orderedEntries.push(buildAccountCard(acc))
      continue
    }

    const groupKey = normalizePlanLabel(acc)
    const group = groupedAccounts.get(groupKey)

    if (group) {
      group.push(acc)
      continue
    }

    groupedAccounts.set(groupKey, [acc])
    orderedEntries.push({ kind: 'group', groupKey })
  }

  return orderedEntries.map((entry) =>
    'kind' in entry
      ? buildAggregateCard(entry.groupKey, groupedAccounts.get(entry.groupKey) ?? [])
      : entry
  )
})

const displayedCards = computed(() => dashboardMode.value === 'aggregate' ? aggregateCards.value : normalCards.value)

const totalLoggedInAccounts = computed(() => stats.value.tasks || stats.value.accounts.length)

const windowValueClass = (window: CardWindow, remainingMode: boolean) => {
  if (!remainingMode) return 'text-gray-300'
  return window.key === 'primary' ? 'text-emerald-400' : 'text-blue-400'
}

const windowBarClass = (window: CardWindow, remainingMode: boolean) => {
  if (window.key === 'primary') {
    return remainingMode
      ? 'bg-emerald-400 shadow-[0_0_8px_rgba(52,211,153,0.5)]'
      : 'bg-emerald-600 shadow-[0_0_8px_rgba(5,150,105,0.5)]'
  }

  return remainingMode
    ? 'bg-blue-400 shadow-[0_0_8px_rgba(96,165,250,0.5)]'
    : 'bg-blue-600 shadow-[0_0_8px_rgba(37,99,235,0.5)]'
}

const formatResetTime = (unixTs: number | null | undefined): string => {
  if (!unixTs) return '—'
  const now = Math.floor(Date.now() / 1000)
  const diff = unixTs - now
  if (diff <= 0) return 'Resets soon'
  const h = Math.floor(diff / 3600)
  const m = Math.floor((diff % 3600) / 60)
  if (h > 0) return `Resets in ${h}h ${m}m`
  return `Resets in ${m}m`
}

const copyKey = async (key: string) => {
  try {
    await navigator.clipboard.writeText(key)
  } catch (e) {
    console.error('Clipboard copy failed:', e)
  }
}

const fetchStatus = async () => {
  try {
    const res = await fetch('http://127.0.0.1:48123/status')
    if (res.ok) {
        const data = await res.json()
        stats.value = data
        message.value = 'Active (Realtime)'
    } else {
        message.value = 'Data Error'
    }
  } catch (e) {
    message.value = 'Disconnected'
  }
}

const addAccount = async () => {
  if (!newAccount.value.name || !newAccount.value.key) return
  await fetch('http://127.0.0.1:48123/accounts', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(newAccount.value)
  })
  showAddModal.value = false
  newAccount.value = { name: '', key: '' }
  fetchStatus()
}

const deleteAccount = async (id: string) => {
  await fetch(`http://127.0.0.1:48123/accounts/${id}`, { method: 'DELETE' })
  fetchStatus()
}

const startEdit = (acc: any) => {
  editingId.value = acc.id
  editName.value = acc.name
}

const finishEdit = async (id: string) => {
  if (!editingId.value) return
  const newName = editName.value.trim()
  editingId.value = null
  if (newName) {
    await fetch(`http://127.0.0.1:48123/accounts/${id}/rename`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: newName })
    })
    fetchStatus()
  }
}

const forceRefresh = async () => {
  if (isSyncing.value) return
  isSyncing.value = true
  await fetch('http://127.0.0.1:48123/refresh', { method: 'POST' })
  setTimeout(() => {
    isSyncing.value = false
  }, 2500)
}

const openSettings = async () => {
  try {
    const res = await fetch('http://127.0.0.1:48123/settings')
    if (res.ok) {
      settings.value = await res.json()
    }
  } catch (e) {
    console.error(e)
  }
  showSettingsModal.value = true
}

const saveSettings = async () => {
  await fetch('http://127.0.0.1:48123/settings', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      polling_enabled: settings.value.polling_enabled,
      polling_interval_secs: Number(settings.value.polling_interval_secs),
      default_quota_view: settings.value.default_quota_view
    })
  })
  showSettingsModal.value = false
}

onMounted(() => {
  fetchStatus()
  statusInterval = window.setInterval(fetchStatus, 1000)
})

onUnmounted(() => {
  if (statusInterval) clearInterval(statusInterval)
})

const minimize = () => (window as any).electronAPI.minimizeWindow()
const maximize = () => (window as any).electronAPI.maximizeWindow()
const close = () => (window as any).electronAPI.closeWindow()

const openDataFolder = () => {
  (window as any).electronAPI.openUserDataFolder()
}
</script>

<template>
  <div class="w-screen h-screen bg-[#202020] text-gray-100 flex flex-col font-['Segoe_UI',system-ui,sans-serif] border border-[#3a3a3a] rounded-xl overflow-hidden shadow-2xl relative select-none">
    
    <!-- Windows 11 顶部拖拽栏 -->
    <div class="h-8 w-full flex items-center justify-between z-10" style="-webkit-app-region: drag">
      <!-- 窗口标题 -->
      <div class="flex items-center space-x-3 px-4">
        <div class="w-3 h-3 bg-blue-500 rounded-sm"></div>
        <span class="text-xs text-gray-300">Focus Flow System</span>
      </div>

      <!-- Windows 控制按钮 -->
      <div class="flex h-full" style="-webkit-app-region: no-drag">
        <button @click="minimize" class="w-11 h-full flex items-center justify-center hover:bg-white/10 transition-colors">
          <svg class="w-2.5 h-2.5 text-gray-300" stroke="currentColor" stroke-width="2" viewBox="0 0 10 10"><line x1="1" y1="5" x2="9" y2="5"></line></svg>
        </button>
        <button @click="maximize" class="w-11 h-full flex items-center justify-center hover:bg-white/10 transition-colors">
          <svg class="w-2.5 h-2.5 text-gray-300" stroke="currentColor" stroke-width="1.5" fill="none" viewBox="0 0 10 10"><rect x="1" y="1" width="8" height="8"></rect></svg>
        </button>
        <button @click="close" class="w-11 h-full flex items-center justify-center hover:bg-[#e81123] hover:text-white transition-colors group">
          <svg class="w-2.5 h-2.5 text-gray-300 group-hover:text-white" stroke="currentColor" stroke-width="1.5" viewBox="0 0 10 10"><path d="M1,1 L9,9 M9,1 L1,9"></path></svg>
        </button>
      </div>
    </div>

    <div class="flex-1 flex overflow-hidden bg-[#202020]">
      <!-- 侧边栏 -->
      <div class="w-64 bg-[#272727] border-r border-[#3a3a3a] flex flex-col p-4 z-10 shadow-sm relative">
        <div class="mb-8 mt-4 pl-3">
          <h1 class="text-2xl font-bold tracking-tight text-white mb-1">Focus Flow</h1>
          <p class="text-gray-400 text-xs">Windows API Monitor</p>
        </div>

        <nav class="flex-1 space-y-2">
          <button class="w-full flex items-center space-x-3 bg-white/5 text-gray-100 px-3 py-2.5 rounded-md transition-colors relative">
            <div class="absolute left-0 top-1/2 -translate-y-1/2 w-1 h-4 bg-blue-500 rounded-r-md"></div>
            <span class="text-blue-400 pl-1">&#9889;</span> 
            <span class="text-sm">Dashboard</span>
          </button>
          
          <button @click="openSettings" class="w-full flex items-center space-x-3 hover:bg-white/5 text-gray-400 hover:text-white px-3 py-2.5 rounded-md transition-colors">
            <span class="pl-1">&#9881;</span> 
            <span class="text-sm">Settings</span>
          </button>
        </nav>

        <div class="mt-auto p-4 rounded-lg bg-[#303030] text-xs text-gray-400 flex flex-col border border-[#3c3c3c]">
          <div class="flex justify-between items-center mb-2">
            <span class="font-semibold text-gray-300">Native Rust Endpoint</span>
            
            <button v-if="!message.includes('Active')" @click="fetchStatus()" class="px-2 py-[3px] bg-red-500/10 text-red-400 hover:bg-red-500/20 hover:text-red-300 rounded border border-red-500/20 transition-all flex items-center space-x-1" title="Force Reconnect">
              <svg xmlns="http://www.w3.org/2000/svg" class="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
              </svg>
              <span>Reconnect</span>
            </button>
          </div>
          <div class="flex justify-between items-center">
            <span>Connection</span>
            <div class="flex items-center space-x-2">
              <div class="w-2 h-2 rounded-full" :class="message.includes('Active') ? 'bg-green-500 shadow-[0_0_8px_rgba(34,197,94,0.6)]' : 'bg-red-500'"></div>
              <span class="font-medium" :class="message.includes('Active') ? 'text-green-400' : 'text-red-400'">{{ message.includes('Active') ? 'Running' : 'Offline' }}</span>
            </div>
          </div>
        </div>
      </div>

      <!-- 右侧内容 -->
      <div class="flex-1 p-8 overflow-y-auto bg-[#1c1c1c] relative relative">
        
        <div class="flex flex-wrap justify-between items-end gap-4 relative pb-2 border-b border-[#303030] mb-6">
           <div>
             <h2 class="text-2xl font-semibold text-white">API Quota Monitors</h2>
             <div class="mt-2 flex flex-wrap items-center gap-2 text-xs text-gray-400">
               <span class="px-2.5 py-1 rounded-md bg-[#252525] border border-[#353535]">
                 Accounts:
                 <span class="ml-1 font-semibold text-white">{{ totalLoggedInAccounts }}</span>
               </span>
             </div>
           </div>
           <div class="flex items-center gap-3">
              <div class="flex items-center bg-[#252525] border border-[#3a3a3a] rounded-md p-1">
                <button
                  @click="dashboardMode = 'normal'"
                 class="px-3 py-1 text-xs font-semibold rounded transition-colors"
                 :class="dashboardMode === 'normal' ? 'bg-blue-600 text-white' : 'text-gray-400 hover:text-white hover:bg-white/5'"
               >
                 Normal
               </button>
               <button
                 @click="dashboardMode = 'aggregate'"
                 class="px-3 py-1 text-xs font-semibold rounded transition-colors"
                 :class="dashboardMode === 'aggregate' ? 'bg-blue-600 text-white' : 'text-gray-400 hover:text-white hover:bg-white/5'"
               >
                 Aggregate
               </button>
              </div>
              <button @click="showAddModal = true" class="text-sm bg-blue-600 hover:bg-blue-500 text-white px-4 py-1.5 rounded transition-colors shadow-md">+ Add API Session</button>
           </div>
        </div>

        <template v-if="stats.accounts && stats.accounts.length === 0">
          <div class="w-full bg-[#2b2b2b] border border-[#3e3e3e] p-8 rounded-lg flex flex-col items-center justify-center cursor-default h-64 mt-4">
            <span class="text-4xl mb-4">&#128273;</span>
            <p class="text-gray-400 font-medium">No accounts monitored. Add a JWT or auth.json to track API usage.</p>
          </div>
        </template>
        
        <template v-else>
          <div class="grid xl:grid-cols-2 gap-6 items-start">
            <div v-for="card in displayedCards" :key="card.id" class="bg-[#2b2b2b] border border-[#3e3e3e] rounded-lg shadow-sm relative group flex flex-col overflow-hidden">
              
              <div class="p-6 pb-5 relative">
                <div v-if="card.source === 'account' && card.account" class="absolute top-4 right-4 flex items-center space-x-2 opacity-0 group-hover:opacity-100 transition-opacity z-10">
                  <button v-if="card.account.key.trim().startsWith('{')" @click="copyKey(card.account.key)" class="text-green-400 bg-green-400/10 hover:bg-green-400/20 px-2 py-1 rounded text-xs border border-green-400/20 transition-colors" title="Copy FULL auth.json array">
                    COPY JSON
                  </button>
                  <button @click="toggleMode(card.id)" class="text-blue-400 bg-blue-400/10 hover:bg-blue-400/20 px-2 py-1 rounded text-xs border border-blue-400/20 transition-colors" title="Toggle Used / Remaining Quota">
                    SWITCH
                  </button>
                  <button @click="deleteAccount(card.account.id)" class="text-red-500 bg-red-500/10 hover:bg-red-500/20 px-2 py-1 rounded text-xs border border-red-500/20 transition-colors">
                    DELETE
                  </button>
                </div>
                <div class="flex items-center space-x-3 mb-4 pr-16 border-l-2 border-transparent hover:border-blue-500 pl-1 transition-all">
                  <div class="w-10 h-10 rounded-full bg-blue-500/10 text-blue-500 flex flex-shrink-0 items-center justify-center text-xl shadow-inner">&#128100;</div>
                  <div class="min-w-0 flex-1">
                      <div class="flex items-center gap-2" v-if="card.source === 'account' && card.account && editingId === card.account.id">
                        <input type="text" v-model="editName" @blur="finishEdit(card.account.id)" @keyup.enter="finishEdit(card.account.id)" class="w-full text-white font-medium text-lg bg-[#303030] outline-none border border-blue-500 rounded px-1.5 py-0.5" autofocus />
                      </div>
                      <h3 v-else class="text-white font-medium text-lg leading-tight flex items-center gap-2 truncate group/name">
                        <span class="truncate">{{ card.name }}</span>
                        <button v-if="card.source === 'account' && card.account" @click="startEdit(card.account)" class="opacity-0 group-hover/name:opacity-100 text-gray-400 hover:text-white transition-opacity shrink-0">
                          <svg xmlns="http://www.w3.org/2000/svg" class="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"></path><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"></path></svg>
                        </button>
                        <span v-if="card.source === 'account' && card.account" class="flex-shrink-0 px-1.5 py-0.5 rounded-full bg-[#303030] text-[10px] text-gray-400 font-mono tracking-wider ml-auto">{{ card.account.id.slice(-4) }}</span>
                        <span v-else class="flex-shrink-0 px-1.5 py-0.5 rounded-full bg-[#303030] text-[10px] text-gray-400 font-mono tracking-wider ml-auto">{{ formatAccountCount(card.memberCount) }}</span>
                      </h3>
                      <p v-if="card.source === 'account' && card.account" class="text-xs text-gray-500 mt-1 font-mono bg-black/20 inline-block px-1.5 py-0.5 rounded truncate max-w-full" :title="card.account.key">{{ displayKey(card.account.key) }}</p>
                      <p v-else class="text-xs text-gray-500 mt-1 font-mono bg-black/20 inline-block px-1.5 py-0.5 rounded truncate max-w-full">Weighted aggregate</p>
                  </div>
                </div>
                
                <div class="flex justify-between items-end mt-4 bg-[#202020] p-4 rounded-md border border-[#303030]">
                  <div>
                      <span class="text-xs text-gray-500 block mb-1">State</span>
                      <span :class="card.status === 'Active' ? 'text-green-400' : (card.status.includes('Error') || card.status.includes('Invalid') ? 'text-red-500' : 'text-yellow-500')" class="text-xs font-semibold uppercase tracking-wider">{{ card.status }}</span>
                  </div>
                  <div class="text-right">
                      <span class="text-xs text-gray-500 block mb-0.5">{{ card.source === 'aggregate' ? 'Plan' : 'Used Quota' }}</span>
                      <span class="text-2xl font-semibold tracking-tight text-white">{{ card.usageLabel || '$0.00' }}</span>
                  </div>
                </div>

                <div class="mt-4 space-y-3 pt-2" v-if="card.status === 'Active'">
                  <div v-for="(window, index) in card.windows" :key="`${card.id}-${window.key}`" :class="index > 0 ? 'pt-1' : ''">
                    <div class="flex justify-between items-center text-[11px] mb-1.5">
                      <span class="text-gray-400 tracking-wide">{{ window.label }} {{ isRemainingMode(card.id) ? 'remaining' : 'limit' }}</span>
                      <span class="font-mono" :class="windowValueClass(window, isRemainingMode(card.id))">
                        {{ isRemainingMode(card.id) ? (100 - window.used).toFixed(1) : window.used.toFixed(1) }}%
                      </span>
                    </div>
                    <div class="w-full h-1.5 bg-[#303030] rounded-full overflow-hidden">
                      <div class="h-full rounded-full transition-all duration-500" 
                          :class="windowBarClass(window, isRemainingMode(card.id))"
                          :style="{ width: Math.min(100, Math.max(0, isRemainingMode(card.id) ? 100 - window.used : window.used)) + '%' }"></div>
                    </div>
                    <div class="text-[10px] text-gray-500 mt-1 text-right font-mono">{{ formatResetTime(window.resetAt) }}</div>
                  </div>
                </div>
              </div>

              <!-- 卡片底部的刷新按钮区 -->
              <button @click="forceRefresh" :disabled="isSyncing" class="w-full border-t border-[#3e3e3e] py-2.5 text-xs font-semibold uppercase tracking-wider flex items-center justify-center gap-1.5 transition-colors"
                      :class="isSyncing ? 'text-blue-500 bg-[#252525]' : 'bg-[#202020] hover:bg-[#252525] text-gray-400 hover:text-blue-400 group'">
                <svg xmlns="http://www.w3.org/2000/svg" class="w-3.5 h-3.5 transition-transform duration-500" 
                     :class="isSyncing ? 'animate-spin' : 'group-active:rotate-180'"
                     viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M21 2v6h-6"></path>
                  <path d="M3 12a9 9 0 1 0 2.81-6.53L3 8"></path>
                  <path d="M3 22v-6h6"></path>
                  <path d="M21 12a9 9 0 1 0-2.81 6.53L21 16"></path>
                </svg>
                {{ isSyncing ? 'Syncing...' : 'Sync Data Now' }}
              </button>
              
            </div>
          </div>
        </template>

      </div>
    </div>
  </div>

  <!-- Add Key Modal popup -->
  <div v-if="showAddModal" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm" style="-webkit-app-region: no-drag">
    <div class="bg-[#2b2b2b] border border-[#444] w-[450px] rounded-xl p-6 shadow-2xl relative">
      <h3 class="text-lg font-semibold mb-6 text-white border-b border-[#444] pb-2">Track New Account</h3>
      <div class="space-y-5">
        <div>
          <label class="block text-xs font-semibold text-gray-300 mb-1.5 uppercase tracking-wide">Display Name</label>
          <input v-model="newAccount.name" type="text" placeholder="e.g. Personal Codex / Work Acc" class="w-full bg-[#1c1c1c] border border-[#444] rounded text-sm p-2.5 text-white outline-none focus:border-blue-500 transition-colors" />
        </div>
        <div>
          <label class="block text-xs font-semibold text-gray-300 mb-1.5 uppercase tracking-wide">JWT / sess- / auth.json</label>
          <input v-model="newAccount.key" type="password" placeholder="Paste credential string here" class="w-full bg-[#1c1c1c] border border-[#444] rounded text-sm p-2.5 text-white outline-none focus:border-blue-500 transition-colors" />
        </div>
        <div class="flex space-x-3 pt-4 border-t border-[#444]">
          <button @click="addAccount" class="flex-1 bg-blue-600 hover:bg-blue-500 text-white font-medium text-sm rounded py-2 transition-colors duration-200">Save Credentials</button>
          <button @click="showAddModal = false" class="flex-1 bg-[#444] hover:bg-[#555] text-white font-medium text-sm rounded py-2 transition-colors duration-200">Cancel</button>
        </div>
      </div>
    </div>
  </div>

  <!-- Settings Modal popup -->
  <div v-if="showSettingsModal" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm" style="-webkit-app-region: no-drag">
    <div class="bg-[#2b2b2b] border border-[#444] w-[450px] rounded-xl p-6 shadow-2xl relative">
      <h3 class="text-lg font-semibold mb-6 text-white border-b border-[#444] pb-2">Global Tracker Settings</h3>
      <div class="space-y-5">
        <label class="flex items-center space-x-3 cursor-pointer group">
          <div class="relative">
            <input type="checkbox" v-model="settings.polling_enabled" class="sr-only">
            <div class="block w-10 h-6 rounded-full transition-colors duration-300" :class="settings.polling_enabled ? 'bg-blue-500' : 'bg-gray-600'"></div>
            <div class="dot absolute left-1 top-1 bg-white w-4 h-4 rounded-full transition-transform duration-300" :class="settings.polling_enabled ? 'translate-x-4' : ''"></div>
          </div>
          <span class="text-sm font-medium text-gray-300 group-hover:text-white transition-colors">Enable Background Server Polling</span>
        </label>
        
        <div :class="{'opacity-50 pointer-events-none': !settings.polling_enabled}" class="transition-opacity">
          <label class="block text-xs font-semibold text-gray-400 mb-1.5 uppercase tracking-wide">Interval (Seconds)</label>
          <input v-model="settings.polling_interval_secs" type="number" min="5" max="3600" class="w-full bg-[#1c1c1c] border border-[#444] rounded text-sm p-2.5 text-white outline-none focus:border-blue-500 transition-colors" />
          <p class="text-xs text-gray-500 mt-2">How frequently the invisible Rust backend contacts OpenAI WHAM APIs.</p>
        </div>

        <div>
          <label class="block text-xs font-semibold text-gray-400 mb-1.5 uppercase tracking-wide">Default Quota View</label>
          <select v-model="settings.default_quota_view" class="w-full bg-[#1c1c1c] border border-[#444] rounded text-sm p-2.5 text-white outline-none focus:border-blue-500 transition-colors">
            <option value="remaining">Remaining</option>
            <option value="limit">Used</option>
          </select>
          <p class="text-xs text-gray-500 mt-2">Sets the default progress display for each account card.</p>
        </div>

        <div class="pt-2">
          <button @click="openDataFolder" class="text-xs text-blue-400 hover:text-blue-300 transition-colors flex items-center gap-1.5 underline-offset-4 hover:underline">
            <svg xmlns="http://www.w3.org/2000/svg" class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
            </svg>
            Open Local Data Folder (Backups)
          </button>
        </div>

        <div class="flex space-x-3 pt-4 border-t border-[#444]">
          <button @click="saveSettings" class="flex-1 bg-blue-600 hover:bg-blue-500 text-white font-medium text-sm rounded py-2 transition-colors duration-200">Apply Changes</button>
          <button @click="showSettingsModal = false" class="flex-1 bg-[#444] hover:bg-[#555] text-white font-medium text-sm rounded py-2 transition-colors duration-200">Cancel</button>
        </div>
      </div>
    </div>
  </div>

</template>

<style>
::-webkit-scrollbar {
  width: 0px;
  background: transparent;
}
</style>
