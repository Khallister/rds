// Utility functions with complex dependencies
import type { User } from '@/types';
import { validateEmail } from '@/utils/validation';
import { API_ENDPOINTS } from '~/config';

export function formatDate(date: Date): string {
  return date.toLocaleDateString('en-US', {
    year: 'numeric',
    month: 'long',
    day: 'numeric'
  });
}

export function calculateSum(numbers: number[]): number {
  return numbers.reduce((sum, num) => sum + num, 0);
}

export function processUser(user: User): Promise<User> {
  if (!validateEmail(user.email)) {
    throw new Error('Invalid email address');
  }
  
  return fetch(`${API_ENDPOINTS.users}/${user.id}`)
    .then(response => response.json())
    .then(data => ({ ...user, ...data }));
}

export function createUserSummary(users: User[]): string {
  const totalUsers = users.length;
  const activeUsers = users.filter(u => u.active).length;
  const formattedDate = formatDate(new Date());
  
  return `Summary as of ${formattedDate}: ${activeUsers}/${totalUsers} users active`;
}
