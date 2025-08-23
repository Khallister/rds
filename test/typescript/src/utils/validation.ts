// Validation utilities with circular import test
import { formatDate } from "@/utils";
import type { User } from "@/types/user";

export function validateEmail(email: string): boolean {
  const emailRegex = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
  return emailRegex.test(email);
}

export function validateUser(user: User): { valid: boolean; errors: string[] } {
  const errors: string[] = [];

  if (!user.name || user.name.trim().length === 0) {
    errors.push("Name is required");
  }

  if (!validateEmail(user.email)) {
    errors.push("Invalid email format");
  }

  if (user.age && (user.age < 0 || user.age > 150)) {
    errors.push("Age must be between 0 and 150");
  }

  console.log(`Validating user on ${formatDate(new Date())}`);

  return {
    valid: errors.length === 0,
    errors,
  };
}

export function sanitizeInput(input: string): string {
  return input
    .trim()
    .replace(/<script\b[^<]*(?:(?!<\/script>)<[^<]*)*<\/script>/gi, "");
}

// Alias for compatibility
export const validateInput = sanitizeInput;
