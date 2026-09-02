export function validatePassword(
  password: string,
  confirmPassword: string,
  isInitializing: boolean,
): string | null {
  if (!password) return "Password is required.";
  if (isInitializing && password !== confirmPassword) return "Passwords do not match.";
  return null;
}
