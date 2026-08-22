export interface Account {
  id: string;
  name: string;
  account_type: string;
  subtype: string;
  institution: string;
  currency: string;
  created_at: string;
}

export const ACCOUNT_TYPES = [
  "depository",
  "credit",
  "loan",
  "investment",
  "other",
] as const;

export type AccountType = (typeof ACCOUNT_TYPES)[number];
