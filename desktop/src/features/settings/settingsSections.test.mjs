import assert from "node:assert/strict";
import test from "node:test";

import {
  DEFAULT_SETTINGS_SECTION,
  isSettingsSection,
} from "@/features/settings/settingsSections";

test("settings route validation accepts the wallet section", () => {
  assert.equal(isSettingsSection("lightning-wallet"), true);
});

test("settings route validation rejects unknown sections", () => {
  assert.equal(isSettingsSection("wallet"), false);
  assert.equal(isSettingsSection(DEFAULT_SETTINGS_SECTION), true);
});
