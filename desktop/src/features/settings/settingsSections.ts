export type SettingsSection =
  | "profile"
  | "notifications"
  | "lightning-wallet"
  | "experimental"
  | "agents"
  | "channel-templates"
  | "compute"
  | "appearance"
  | "shortcuts"
  | "community-members"
  | "moderation"
  | "custom-emoji"
  | "local-archive"
  | "mobile"
  | "updates"
  | "doctor";

export const DEFAULT_SETTINGS_SECTION: SettingsSection = "profile";

const SETTINGS_SECTION_VALUES: readonly SettingsSection[] = [
  "profile",
  "notifications",
  "lightning-wallet",
  "experimental",
  "agents",
  "channel-templates",
  "compute",
  "appearance",
  "shortcuts",
  "community-members",
  "moderation",
  "custom-emoji",
  "local-archive",
  "mobile",
  "updates",
  "doctor",
];

export function isSettingsSection(value: unknown): value is SettingsSection {
  return (
    typeof value === "string" &&
    (SETTINGS_SECTION_VALUES as readonly string[]).includes(value)
  );
}
