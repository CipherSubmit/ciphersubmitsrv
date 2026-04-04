<script setup lang="ts">
import { darkTheme, dateZhCN, NConfigProvider, NDialogProvider, NMessageProvider, zhCN } from 'naive-ui';

const themeOverrides = {
  common: {
    primaryColor: '#1d4ed8',
    primaryColorHover: '#1e40af',
    primaryColorPressed: '#1e3a8a',
    borderRadius: '14px',
    fontFamily: 'Noto Sans SC, sans-serif',
    fontFamilyMono: 'JetBrains Mono, monospace',
  },
}

const useDarkTheme = false
</script>

<template>
  <NConfigProvider
    :date-locale="dateZhCN"
    :locale="zhCN"
    :theme="useDarkTheme ? darkTheme : null"
    :theme-overrides="themeOverrides"
  >
    <NDialogProvider>
      <NMessageProvider>
        <RouterView />
      </NMessageProvider>
    </NDialogProvider>
  </NConfigProvider>
</template>