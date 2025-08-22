// Configuration constants using tilde alias
export const API_ENDPOINTS = {
  users: '/api/users',
  auth: '/api/auth',
  posts: '/api/posts',
  settings: '/api/settings'
} as const;

export const APP_CONFIG = {
  version: '1.0.0',
  environment: process.env.NODE_ENV || 'development',
  enableDebug: true,
  maxRetries: 3
} as const;
