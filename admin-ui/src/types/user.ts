export interface AuthUser {
  id: string;
  email: string;
  role: string;
  password?: string;
  scope?: string;
  metadata?: object;
}
