export interface UserProfile {
  name: string;
  email: string;
  avatarUrl: string | null;
  plan: string;
  tier: string;
  licenseKey: string;
  activatedAt: string;
  status: string;
}

const DEFAULT_PROFILE: UserProfile = {
  name: "Developer",
  email: "developer@bhippi.local",
  avatarUrl: null,
  plan: "Lifetime Activation",
  tier: "Bhippi Pro Edition",
  licenseKey: "BHP-9842-LFTM-8821-V1PRO",
  activatedAt: "Permanent Access",
  status: "Active · Unlimited",
};

const STORAGE_KEY = "bhippi-user-profile";
const PROFILE_CHANGE_EVENT = "bhippi-profile-changed";

export function getProfile(): UserProfile {
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULT_PROFILE;
    return { ...DEFAULT_PROFILE, ...JSON.parse(raw) };
  } catch {
    return DEFAULT_PROFILE;
  }
}

export function saveProfile(partial: Partial<UserProfile>): UserProfile {
  const current = getProfile();
  const next: UserProfile = { ...current, ...partial };
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
    window.dispatchEvent(new CustomEvent(PROFILE_CHANGE_EVENT, { detail: next }));
  } catch (err) {
    console.error("Failed to save profile:", err);
  }
  return next;
}

export function onProfileChange(callback: (profile: UserProfile) => void): () => void {
  const handler = (e: Event) => {
    const custom = e as CustomEvent<UserProfile>;
    callback(custom.detail || getProfile());
  };
  window.addEventListener(PROFILE_CHANGE_EVENT, handler);
  window.addEventListener("storage", () => callback(getProfile()));
  return () => {
    window.removeEventListener(PROFILE_CHANGE_EVENT, handler);
  };
}

export function maskLicenseKey(key: string): string {
  if (!key) return "••••-••••-••••-••••";
  const parts = key.split("-");
  if (parts.length >= 4) {
    return `${parts[0]}-••••-••••-${parts[parts.length - 1]}`;
  }
  return `${key.slice(0, 4)}••••••••${key.slice(-4)}`;
}
