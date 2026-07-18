// router/index.ts
import { createRouter, createWebHistory } from 'vue-router';
import { useAuthStore } from '@/stores/auth';

const routes = [
  { path: '/', redirect: '/spaces'},
  { path: '/login', name: 'login', component: () => import('@/views/LoginView.vue') },
  {
    path: '/spaces',
    name: 'spaces',
    component: () => import('@/views/MainView.vue'),
    meta: { requiresAuth: true },
  },
  {
    path: '/spaces/:spaceId',
    name: 'space',
    component: () => import('@/views/MainView.vue'),
    meta: { requiresAuth: true },
  },
  {
    path: '/spaces/:spaceId/rooms/:roomId',
    name: 'room',
    component: () => import('@/views/MainView.vue'),
    meta: { requiresAuth: true },
  },
  {
    path: '/spaces/:spaceId/calls/:roomId',
    name: 'call',
    component: () => import('@/views/CallView.vue'),
    meta: { requiresAuth: true },
  },
];

const router = createRouter({ history: createWebHistory(), routes });

router.beforeEach((to) => {
  const auth = useAuthStore();
  if (to.meta.requiresAuth && !auth.isLoggedIn) {
    return { name: 'login' };
  }
});

export default router;
