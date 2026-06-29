import path from "node:path";
import { fileURLToPath } from "node:url";
import { runFileSizeCheck } from "../../scripts/check-file-sizes-core.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(__dirname, "..");

const MAX_LINES = 1000;

const rules = [
  { root: "src-tauri/src", extensions: new Set([".rs"]), maxLines: MAX_LINES },
  {
    root: "src/app",
    extensions: new Set([".ts", ".tsx"]),
    maxLines: MAX_LINES,
  },
  {
    root: "src/features",
    extensions: new Set([".ts", ".tsx"]),
    maxLines: MAX_LINES,
  },
  {
    root: "src/shared/api",
    extensions: new Set([".ts", ".tsx"]),
    maxLines: MAX_LINES,
  },
  {
    root: "src/shared/context",
    extensions: new Set([".ts", ".tsx"]),
    maxLines: MAX_LINES,
  },
  {
    root: "src/shared/lib",
    extensions: new Set([".ts", ".tsx"]),
    maxLines: MAX_LINES,
  },
  {
    root: "src/shared/ui",
    extensions: new Set([".ts", ".tsx"]),
    maxLines: MAX_LINES,
  },
  {
    root: "src/shared/styles",
    extensions: new Set([".css"]),
    maxLines: MAX_LINES,
  },
];

// TEMP — these files exceed the 1000-line limit and are queued to be split.
// Do not add to this list; split the file instead. Remove each entry as its
// file is broken up. Tracked as a follow-up.
const overrides = new Map([
  // persona-events rebase: build_deploy_payload threads `state` for the
  // read-time relay-URL workspace fallback while keeping the create-time env
  // pin (the credential-leak guard). Load-bearing feature growth from the
  // rebase, queued to split with the rest of this list.
  // persona-refresh-on-spawn: re-snapshot + retain_managed_agent_pending call
  // in start_local_agent_with_preflight adds ~23 lines. Queued to split.
  ["src-tauri/src/commands/agents.rs", 1437],
  // Bitcoin branch channel payment policy plumbing, queued to split with the
  // rest of this list. Mainline channel metadata tracing adds a small amount
  // while preserving the branch wallet metadata fetch.
  ["src-tauri/src/commands/channels.rs", 1079],
  // Bitcoin branch event handling keeps wallet/payment events in the desktop
  // event module. Queued to split with the rest of this list.
  ["src-tauri/src/events.rs", 1234],
  // Residual repos_dir integration in ensure_nest_at: REPOS is provisioned
  // outside NEST_DIRS (it may be a symlink), so it needs its own create +
  // chmod-only-when-real-dir handling plus integration test coverage. The
  // self-contained repos_dir functions and their unit tests live in repos.rs;
  // this is the seam that must stay in nest.rs. Approved override; still queued
  // to split with the rest of this list.
  ["src-tauri/src/managed_agents/nest.rs", 1450],
  // harness-persona-sync: persona-runtime resolution threaded into the spawn
  // path here. Load-bearing feature growth; queued to split in the resolver
  // unify refactor followup. +26 for resolve_effective_prompt_model_provider
  // re-introduced after 826d735fe removal (config-bridge caller still needs it).
  // PGID resolution helper + PID-recycling safety guard added for orphan sweep.
  // Mainline restore/watchdog handling adds the latest overage.
  ["src-tauri/src/managed_agents/runtime.rs", 2183],
  // Phase-2 inbound reconcile + review-fix cycle: reconcile_inbound_persona_event
  // dispatches 30175/30176/30177 inbound plus kind:5 tombstone consume
  // (reconcile_inbound_tombstone), the two apply_inbound_* fns, the
  // event_d_tag/parse_deletion_coordinate helpers, and the preserve/overwrite +
  // secret-injection + tombstone test coverage. Load-bearing feature growth,
  // queued to split with the list. The two `agents-data-changed` emits (live
  // UI refresh on inbound reconcile + tombstone) add the latest growth.
  ["src-tauri/src/commands/personas.rs", 1279],
  // Local wallet broker bridge remains branch-local for bitcoin dev work.
  // Queued to split with the rest of this list.
  ["src-tauri/src/wallet/broker.rs", 1482],
  // Bitcoin wallet provider integrations are branch-local and need a split.
  ["src-tauri/src/wallet/cashu.rs", 1289],
  ["src-tauri/src/wallet/hive.rs", 1891],
  // applyWorkspace reposDir parameter plus the validateReposDir binding,
  // threaded through Tauri invokes for configurable repos_dir, plus the
  // harness-persona-sync `harnessOverride` create-input bit — load-bearing
  // parameter plumbing, not generic debt growth. Approved override; still
  // queued to split.
  ["src/shared/api/tauri.ts", 1235],
  // harness-persona-sync feature growth, queued to split in the resolver-unify
  // refactor followup. discovery.rs is dominated by the new test module
  // (the effective_agent_command / divergent / create-time override matrix);
  // alias-preservation coverage extends that matrix so create-time persona
  // agents keep an installed runtime alias when the primary command is absent.
  // Load-bearing, not generic debt.
  // config-bridge: schema-driven field extraction adds ~26 lines. Queued to split.
  ["src-tauri/src/managed_agents/discovery.rs", 1111],
  // migration_tests.rs carries the harness-sync migration coverage plus the
  // patch_json_records owner-only writeback regression test (SECURITY.md:90
  // crash-safe 0o600 fallback). Load-bearing security + feature coverage, not
  // generic debt growth. Approved override; still queued to split.
  ["src-tauri/src/migration_tests.rs", 1410],
  ["src-tauri/src/nostr_convert.rs", 1126],
  ["src/shared/api/relayClientSession.ts", 1037],
  ["src-tauri/src/migration.rs", 1449],
  // persona-events rebase: boot-time event-sync wiring (run_boot_migrations
  // syncs team-dir edits before all personas.json readers; run_event_sync
  // signs the persona/team retention events post-identity) layered on top of
  // main's growth. Load-bearing feature growth, queued to split with the list.
  // Bitcoin branch startup also starts/prewarms the wallet provider.
  ["src-tauri/src/lib.rs", 1092],
  // Branch payment annotation plumbing plus mainline composer controls. Queued
  // to split with the rest of this list.
  ["src/features/messages/ui/MessageComposer.tsx", 1141],
  // onMarkRead + isUnread prop threading (mirrors the onMarkUnread prop
  // already here) for the single-toggle mark-read/unread menu item — a small
  // overage from load-bearing per-message plumbing, not generic debt growth.
  // Approved override; still queued to split with the rest of this list.
  ["src/features/messages/ui/MessageThreadPanel.tsx", 1010],
  // Mainline channel sheet split plus restored bitcoin branch wallet/gift
  // entry points put this just over the limit. Queued to split.
  ["src/features/channels/ui/ChannelManagementSheet.tsx", 1050],
  // Mainline split-panel support plus bitcoin branch paid-join/walletbot
  // plumbing. Queued to split.
  ["src/features/channels/ui/ChannelPane.tsx", 1023],
  // Mainline auxiliary panel state plus bitcoin branch paid-join/walletbot
  // header totals. Queued to split with the channel pane follow-up.
  ["src/features/channels/ui/ChannelScreen.tsx", 1063],
  // Mainline members panel polish carries a small overage pending component
  // extraction.
  ["src/features/channels/ui/MembersSidebar.tsx", 1007],
  // Bitcoin branch wallet settings UI is branch-local and needs extraction.
  ["src/features/settings/ui/LightningWalletSettingsCard.tsx", 1245],
  // AgentConfigPanel footer fold into ProfileFieldGroup for the config-bridge
  // panel — a small overage from load-bearing UI plumbing, not generic debt
  // growth. Approved override; still queued to split with the rest of this list.
  // +135 for AgentInfoFocusedView/DiagnosticsFocusedView/ChannelsFocusedView
  // props restored after 826d735fe removal (UserProfilePanel.tsx still needs them).
  ["src/features/profile/ui/UserProfilePanelSections.tsx", 1140],
  // Mainline archive/persona panel split plus bitcoin branch payment form query.
  // Queued to split once the profile panel branch work settles.
  ["src/features/profile/ui/UserProfilePanel.tsx", 1014],
  // PersistBackend enum + marker-on-keyring-success plumbing and its three
  // fail-closed regression tests (silent identity rotation on keyring outage).
  // A small overage from load-bearing security plumbing on a file already at
  // 893 lines, not generic debt growth. Approved override; still queued to split.
  // Mainline session config cache is kept alongside the wallet broker state.
  ["src-tauri/src/app_state.rs", 1039],
  // multi-slot splitting + no-op suppression (#1309): the ReadStateManager
  // class grew from ~700 lines to ~1019 with the addition of
  // splitContextsIntoBudgetedSlots (pure fn + 5 tests), publishSplitSlots,
  // publishOneSlot, deleteExtraSlots, and the no-op suppression integration
  // test. Load-bearing feature growth, queued to split publishSplitSlots path
  // into readStateManagerSplit.ts.
  ["src/features/channels/readState/readStateManager.ts", 1030],
  // Shared UI was added to this guard after splitting globals/markdown so
  // large shared renderers cannot grow further while follow-up splits land.
  ["src/shared/ui/markdown.tsx", 2082],
  ["src/shared/ui/VideoPlayer.tsx", 2199],
  ["src/shared/ui/sidebar.tsx", 1042],
]);

await runFileSizeCheck({
  projectRoot,
  rules,
  overrides,
  label: "Desktop",
  scriptPath: "desktop/scripts/check-file-sizes.mjs",
});
