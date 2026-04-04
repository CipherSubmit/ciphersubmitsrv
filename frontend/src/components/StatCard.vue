<script setup lang="ts">
const toneClasses: Record<'blue' | 'green' | 'amber' | 'slate', string> = {
  blue: 'border-t-blue-700',
  green: 'border-t-teal-700',
  amber: 'border-t-amber-700',
  slate: 'border-t-slate-600',
}

defineProps<{
  title: string
  value: string | number
  hint?: string
  tone?: 'blue' | 'green' | 'amber' | 'slate'
}>()
</script>

<template>
  <article
    class="grid min-h-[132px] gap-2 rounded-3xl border border-t-4 border-slate-200 bg-white/95 p-5 shadow-[0_14px_34px_rgba(15,23,42,0.06)]"
    :class="toneClasses[tone || 'blue']"
  >
    <p class="m-0 text-sm text-slate-600">{{ title }}</p>
    <strong class="font-['Manrope'] text-[2.2rem] leading-none text-slate-950">{{ value }}</strong>
    <span v-if="hint" class="text-sm text-slate-500">{{ hint }}</span>
  </article>
</template>