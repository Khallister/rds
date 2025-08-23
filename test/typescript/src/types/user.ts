export interface User {
  id: number;
  name: string;
  email: string;
  age?: number;
  active: boolean;
  role: "admin" | "user" | "moderator";
  createdAt: Date;
  lastLogin?: Date;
}

export interface ApiResponse<T = any> {
  success: boolean;
  data: T;
  message: string;
  timestamp?: Date;
  errors?: string[];
}
