import { createRouter, createWebHistory, type RouteRecordRaw } from 'vue-router'

import { getStoredToken } from '@/services/http'

const routes: RouteRecordRaw[] = [
  {
    path: '/login',
    name: 'login',
    component: () => import('@/views/LoginView.vue'),
    meta: { public: true },
  },
  {
    path: '/',
    component: () => import('@/layouts/AdminLayout.vue'),
    children: [
      {
        path: '',
        redirect: '/dashboard/overview',
      },
      {
        path: '/dashboard/overview',
        name: 'overview',
        component: () => import('@/views/DashboardView.vue'),
      },
      {
        path: '/dashboard/activity',
        name: 'activity',
        component: () => import('@/views/ActivityView.vue'),
      },
      {
        path: '/dashboard/teacher-keys',
        name: 'teacher-keys',
        component: () => import('@/views/TeacherKeysView.vue'),
      },
      {
        path: '/dashboard/maintenance',
        name: 'maintenance',
        component: () => import('@/views/MaintenanceView.vue'),
      },
    ],
  },
]

const router = createRouter({
  history: createWebHistory(),
  routes,
  scrollBehavior: () => ({ top: 0 }),
})

router.beforeEach((to) => {
  const hasToken = Boolean(getStoredToken())

  if (to.meta.public && hasToken && to.path === '/login') {
    return '/dashboard/overview'
  }

  if (!to.meta.public && !hasToken) {
    return {
      path: '/login',
      query: { redirect: to.fullPath },
    }
  }

  return true
})

export default router
