// Re-export all types from individual files
export * from './user';

// Additional common types
export interface ComponentProps {
  className?: string;
  children?: React.ReactNode;
}

export type Theme = 'light' | 'dark' | 'auto';

export interface AppConfig {
  theme: Theme;
  apiUrl: string;
  enableLogging: boolean;
}
