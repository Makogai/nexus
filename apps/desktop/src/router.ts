import { createRouter, createWebHashHistory } from 'vue-router'

export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/', name: 'home', component: () => import('./views/HomeView.vue') },
    { path: '/projects', name: 'projects', component: () => import('./views/ProjectsView.vue') },
    { path: '/projects/:id', name: 'project', component: () => import('./views/ProjectDetailView.vue'), props: true },
    { path: '/sessions', name: 'sessions', component: () => import('./views/SessionsView.vue') },
    { path: '/profiles', name: 'profiles', component: () => import('./views/ProfilesView.vue') },
    { path: '/settings', name: 'settings', component: () => import('./views/SettingsView.vue') },
    { path: '/:pathMatch(.*)*', redirect: '/' },
  ],
})
