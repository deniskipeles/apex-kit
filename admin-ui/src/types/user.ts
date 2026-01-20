
export interface AuthUser {
  id: string;
  email: string;
  role: string;
  password?: string;
  metadata?: object;
}
