import * as React from "react";

import { EditorContent } from "@tiptap/react";
import { useChannelLinks } from "@/features/messages/lib/useChannelLinks";
import { useComposerAutofocus } from "@/features/messages/lib/useComposerAutofocus";
import type { ChannelSuggestion } from "@/features/messages/lib/useChannelLinks";
import { useDrafts } from "@/features/messages/lib/useDrafts";
import { useEmojiAutocomplete } from "@/features/messages/lib/useEmojiAutocomplete";
import type { EmojiSuggestion } from "@/features/messages/lib/useEmojiAutocomplete";
import {
  buildOutgoingMessage,
  type ImetaMedia,
  stripImetaMediaLines,
} from "@/features/messages/lib/imetaMediaMarkdown";
import { resolveBountyTargetPubkey } from "@/features/messages/lib/messageBounties";

import {
  ALLOWED_MEDIA_TYPES,
  useMediaUpload,
} from "@/features/messages/lib/useMediaUpload";
import { useMentions } from "@/features/messages/lib/useMentions";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import {
  hasMentionClipboardHtml,
  normalizeMentionClipboardHtml,
} from "@/features/messages/lib/normalizeMentionClipboard";
import {
  type AutocompleteEdit,
  useRichTextEditor,
} from "@/features/messages/lib/useRichTextEditor";
import { useTypingBroadcast } from "@/features/messages/useTypingBroadcast";
import { getSproutCodeBlockClipboardText } from "@/shared/lib/codeBlockClipboard";
import { cn } from "@/shared/lib/cn";
import { ChannelAutocomplete } from "./ChannelAutocomplete";
import { ComposerAttachments, DropZoneOverlay } from "./ComposerAttachments";
import { ComposerBountyChip } from "./ComposerBountyChip";
import { ComposerBountyDialog } from "./ComposerBountyDialog";
import { ComposerContextBanner } from "./ComposerContextBanner";
import { ComposerKudosChip } from "./ComposerKudosChip";
import { EmojiAutocomplete } from "./EmojiAutocomplete";
import {
  MentionAutocomplete,
  type MentionSuggestion,
} from "./MentionAutocomplete";
import { MessageComposerToolbar } from "./MessageComposerToolbar";
import { useComposerBounty } from "./useComposerBounty";
import { useComposerKudos } from "./useComposerKudos";

type MessageComposerProps = {
  channelId?: string | null;
  channelName: string;
  containerClassName?: string;
  disabled?: boolean;
  draftKey?: string;
  editTarget?: {
    author: string;
    body: string;
    id: string;
    imetaMedia?: ImetaMedia[];
  } | null;
  isSending?: boolean;
  onCancelEdit?: () => void;
  onCancelReply?: () => void;
  onEditSave?: (content: string, mediaTags?: string[][]) => Promise<void>;
  onSend: (
    content: string,
    mentionPubkeys: string[],
    mediaTags?: string[][],
    options?: { bountyAmountSats?: number | null; kudos?: boolean },
  ) => Promise<void>;
  placeholder?: string;
  profiles?: UserProfileLookup;
  replyTarget?: {
    author: string;
    body: string;
    id: string;
  } | null;
  showTopBorder?: boolean;
  toolbarExtraActions?: React.ReactNode;
  typingParentEventId?: string | null;
  typingRootEventId?: string | null;
};

export function MessageComposer({
  channelId = null,
  channelName,
  containerClassName,
  disabled = false,
  draftKey,
  editTarget = null,
  isSending = false,
  onCancelEdit,
  onCancelReply,
  onEditSave,
  onSend,
  placeholder,
  profiles,
  replyTarget = null,
  showTopBorder = false,
  toolbarExtraActions,
  typingParentEventId = null,
  typingRootEventId = null,
}: MessageComposerProps) {
  const [content, setContent] = React.useState("");
  const contentRef = React.useRef(content);
  contentRef.current = content;

  const [isEmojiPickerOpen, setIsEmojiPickerOpen] = React.useState(false);
  const [isFormattingOpen, setIsFormattingOpen] = React.useState(false);
  const [sendError, setSendError] = React.useState<string | null>(null);

  const handleFormattingToggle = React.useCallback((pressed: boolean) => {
    if (pressed) setIsEmojiPickerOpen(false);
    setIsFormattingOpen(pressed);
  }, []);

  const drafts = useDrafts();
  const effectiveDraftKey = draftKey ?? channelId;
  const previousDraftKeyRef = React.useRef<string | null>(null);
  const effectiveDraftKeyRef = React.useRef(effectiveDraftKey);
  effectiveDraftKeyRef.current = effectiveDraftKey;
  const preEditSnapshotRef = React.useRef<{
    content: string;
    pendingImeta: ImetaMedia[];
  } | null>(null);
  const mentions = useMentions(channelId, undefined, profiles);
  const channelLinks = useChannelLinks();
  const emojiAutocomplete = useEmojiAutocomplete();
  const notifyTyping = useTypingBroadcast(
    channelId,
    typingParentEventId,
    typingRootEventId,
  );

  // We pass a custom setter that both updates React state AND inserts
  // markdown into the Tiptap editor when media upload completes.
  const media = useMediaUpload();

  const disabledRef = React.useRef(disabled);
  const isSendingRef = React.useRef(isSending);
  const isUploadingRef = React.useRef(media.isUploading);
  const onSendRef = React.useRef(onSend);
  const onEditSaveRef = React.useRef(onEditSave);
  const editTargetRef = React.useRef(editTarget);
  disabledRef.current = disabled;
  isSendingRef.current = isSending;
  isUploadingRef.current = media.isUploading;
  onSendRef.current = onSend;
  onEditSaveRef.current = onEditSave;
  editTargetRef.current = editTarget;

  const isAutocompleteOpenRef = React.useRef(false);
  isAutocompleteOpenRef.current =
    mentions.isMentionOpen ||
    channelLinks.isChannelOpen ||
    emojiAutocomplete.isEmojiAutocompleteOpen;

  const submitMessageRef = React.useRef<() => void>(() => {});
  const composerScrollRef = React.useRef<HTMLDivElement>(null);

  const scrollComposerToBottom = React.useCallback(() => {
    window.requestAnimationFrame(() => {
      const scrollElement = composerScrollRef.current;
      if (!scrollElement) return;
      scrollElement.scrollTop = scrollElement.scrollHeight;
    });
  }, []);

  const computedPlaceholder = editTarget
    ? "Edit your message"
    : (placeholder ??
      (replyTarget
        ? `Reply to ${replyTarget.author} in #${channelName}`
        : `Message #${channelName}`));

  const richText = useRichTextEditor({
    placeholder: computedPlaceholder,
    editable: !disabled,
    mentionNames: mentions.knownNames,
    channelNames: channelLinks.knownChannelNames,
    onSubmit: () => submitMessageRef.current(),
    isAutocompleteOpen: isAutocompleteOpenRef,
    onUpdate: ({ markdown, text }) => {
      setContent(markdown);
      contentRef.current = markdown;

      // Bridge to mention/channel/emoji detection hooks. `text` and the
      // cursor are both in plain-text-offset space (treating hardBreak
      // and inter-block boundaries as `\n`).
      const { cursor } = richText.getPlainTextAndCursor();
      mentions.updateMentionQuery(text, cursor);
      channelLinks.updateChannelQuery(text, cursor);
      emojiAutocomplete.updateEmojiQuery(text, cursor);

      if (text.trim().length > 0) {
        notifyTyping();
      }
    },
  });
  const {
    handleGiveKudos: activateKudos,
    isKudosActive,
    kudosDisabled,
    setIsKudosActive,
  } = useComposerKudos({
    disabled,
    editTargetActive: Boolean(editTarget),
    focusEditor: richText.focus,
  });
  const {
    bountyAmountSats,
    bountyDialog,
    bountyDisabled,
    handleAddBounty,
    setBountyAmountSats,
  } = useComposerBounty({
    disabled,
    editTargetActive: Boolean(editTarget),
    focusEditor: richText.focus,
  });

  // biome-ignore lint/correctness/useExhaustiveDependencies: effectiveDraftKey is the sole trigger
  React.useEffect(() => {
    const prevKey = previousDraftKeyRef.current;
    if (prevKey) {
      drafts.persistDraft(prevKey, contentRef.current);
    }
    previousDraftKeyRef.current = effectiveDraftKey;

    const saved = effectiveDraftKey
      ? drafts.loadDraft(effectiveDraftKey)
      : undefined;
    if (saved) {
      setContent(saved.content);
      contentRef.current = saved.content;
      richText.setContent(saved.content);
    } else {
      setContent("");
      contentRef.current = "";
      richText.clearContent();
    }

    media.setPendingImeta([]);
    media.setUploadState({ status: "idle" });
    setIsKudosActive(false);
    setBountyAmountSats(null);
    setSendError(null);
    setIsEmojiPickerOpen(false);
    mentions.clearMentions();
    channelLinks.clearChannels();
    emojiAutocomplete.clearEmojis();

    return () => {
      if (effectiveDraftKey) {
        drafts.persistDraft(effectiveDraftKey, contentRef.current);
      }
    };
  }, [effectiveDraftKey]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: editTarget?.id is the trigger
  React.useEffect(() => {
    if (editTarget) {
      setIsKudosActive(false);
      setBountyAmountSats(null);
      // Snapshot the current draft (text + attachments) so the user's
      // in-flight work survives the edit-mode hijack and is restored on
      // edit-cancel/exit.
      preEditSnapshotRef.current = {
        content: contentRef.current,
        pendingImeta: [...media.pendingImetaRef.current],
      };
      // Strip the trailing `![image|video](url)` lines that correspond to
      // imeta attachments — the user manages those via the attachments row,
      // not via raw markdown in the editor.
      const editableBody = stripImetaMediaLines(
        editTarget.body,
        editTarget.imetaMedia ?? [],
      );
      setContent(editableBody);
      contentRef.current = editableBody;
      richText.setContent(editableBody);
      // Seed the composer's pending-imeta state with the original event's
      // attachments so they show up in `ComposerAttachments` and the user
      // can remove existing ones / add new ones before saving.
      media.setPendingImeta(editTarget.imetaMedia ?? []);
      // Defer focus to the next frame so it runs after any focus-
      // restoration the trigger UI (e.g. the message-row context menu)
      // fires on close. Without this, Radix-style focus-restoration races
      // our call and leaves DOM focus on the message row — global keybinds
      // like Delete then fire there instead of in the editor. `focusEnd`
      // also lands the caret at end of the loaded content.
      const rafId = requestAnimationFrame(() => richText.focusEnd());
      return () => cancelAnimationFrame(rafId);
    } else if (preEditSnapshotRef.current !== null) {
      const { content: restoredContent, pendingImeta: restoredImeta } =
        preEditSnapshotRef.current;
      preEditSnapshotRef.current = null;
      setContent(restoredContent);
      contentRef.current = restoredContent;
      restoredContent
        ? richText.setContent(restoredContent)
        : richText.clearContent();
      media.setPendingImeta(restoredImeta);
    }
  }, [editTarget?.id]);

  // ── Focus on reply ──────────────────────────────────────────────────
  // Use focusPreserve so that re-renders (e.g. new messages arriving in
  // a thread) don't yank the cursor to the end while the user is editing.
  React.useEffect(() => {
    if (!replyTarget || disabled) return;
    richText.focusPreserve();
  }, [disabled, replyTarget, richText.focusPreserve]);

  // ── Autofocus on mount / channel switch ─────────────────────────────
  useComposerAutofocus(richText.focus, effectiveDraftKey, disabled);

  // ── Mention / channel / emoji autocomplete insertion ────────────────
  // Hooks return a plain-text edit descriptor; `replacePlainTextRange`
  // applies it as a single ProseMirror transaction (no markdown round-trip).
  const applyAutocompleteEdit = React.useCallback(
    (edit: AutocompleteEdit) => {
      richText.replacePlainTextRange(
        edit.replaceFromOffset,
        edit.replaceToOffset,
        edit.insertText,
      );
    },
    [richText.replacePlainTextRange],
  );

  const applyMentionInsert = React.useCallback(
    (suggestion: MentionSuggestion) => {
      const { cursor } = richText.getPlainTextAndCursor();
      applyAutocompleteEdit(mentions.insertMention(suggestion, cursor));
    },
    [
      applyAutocompleteEdit,
      mentions.insertMention,
      richText.getPlainTextAndCursor,
    ],
  );

  const applyChannelInsert = React.useCallback(
    (suggestion: ChannelSuggestion) => {
      const { cursor } = richText.getPlainTextAndCursor();
      applyAutocompleteEdit(channelLinks.insertChannel(suggestion, cursor));
    },
    [
      applyAutocompleteEdit,
      channelLinks.insertChannel,
      richText.getPlainTextAndCursor,
    ],
  );

  const applyEmojiInsert = React.useCallback(
    (suggestion: EmojiSuggestion) => {
      const { cursor } = richText.getPlainTextAndCursor();
      applyAutocompleteEdit(emojiAutocomplete.insertEmoji(suggestion, cursor));
    },
    [
      applyAutocompleteEdit,
      emojiAutocomplete.insertEmoji,
      richText.getPlainTextAndCursor,
    ],
  );

  // ── Emoji insertion ─────────────────────────────────────────────────
  const insertEmoji = React.useCallback(
    (emoji: string) => {
      if (!richText.editor) return;
      richText.editor.chain().focus().insertContent(emoji).run();
      setIsEmojiPickerOpen(false);
      mentions.clearMentions();
    },
    [richText.editor, mentions.clearMentions],
  );

  // ── @ mention picker (toolbar button) ───────────────────────────────
  const openMentionPicker = React.useCallback(() => {
    if (!richText.editor) return;
    const { text, cursor } = richText.getPlainTextAndCursor();

    // Check if there's already an @-query in progress
    const beforeCursor = text.slice(0, cursor);
    if (/(?:^|[\s])@[^\s]*$/.test(beforeCursor)) {
      mentions.updateMentionQuery(text, cursor);
      richText.focus();
      return;
    }

    // Insert @ at cursor
    const previousChar = text.slice(0, cursor).slice(-1);
    const prefix =
      cursor > 0 && previousChar && !/\s/.test(previousChar) ? " @" : "@";
    richText.editor.chain().focus().insertContent(prefix).run();
    setIsEmojiPickerOpen(false);

    // Trigger mention detection after inserting @
    const { text: updatedText, cursor: updatedCursor } =
      richText.getPlainTextAndCursor();
    mentions.updateMentionQuery(updatedText, updatedCursor);
  }, [
    richText.editor,
    richText.getPlainTextAndCursor,
    richText.focus,
    mentions.updateMentionQuery,
  ]);

  // ── Submit message ──────────────────────────────────────────────────
  const submitMessage = React.useCallback(async () => {
    const trimmed = contentRef.current.trim();

    // Edit mode
    if (editTargetRef.current && onEditSaveRef.current) {
      if (isSendingRef.current || isUploadingRef.current) return;
      const currentPendingImeta = media.pendingImetaRef.current;
      const hasMedia = currentPendingImeta.length > 0;
      // Empty text + zero attachments is a no-op (don't let edit become an
      // effective deletion).
      if (!trimmed && !hasMedia) return;

      // Build the edit's body + imeta tag set. Coerce `mediaTags ?? []`
      // because edit semantics use `[]` as the explicit "wipe all
      // attachments" signal — the receiver overlay drops imeta when the
      // edit carries an empty (but defined) set.
      const { content: finalContent, mediaTags } = buildOutgoingMessage(
        trimmed,
        currentPendingImeta,
      );

      const savedContent = trimmed;
      const savedImeta = [...currentPendingImeta];
      setContent("");
      contentRef.current = "";
      richText.clearContent();
      media.setPendingImeta([]);
      mentions.clearMentions();
      channelLinks.clearChannels();
      emojiAutocomplete.clearEmojis();
      setIsEmojiPickerOpen(false);

      try {
        await onEditSaveRef.current(finalContent, mediaTags ?? []);
      } catch {
        setContent(savedContent);
        contentRef.current = savedContent;
        richText.setContent(savedContent);
        media.setPendingImeta(savedImeta);
      }
      return;
    }

    // Normal send
    const currentPendingImeta = media.pendingImetaRef.current;
    const hasMedia = currentPendingImeta.length > 0;
    if (
      (!trimmed && !hasMedia) ||
      disabledRef.current ||
      isSendingRef.current ||
      isUploadingRef.current
    ) {
      return;
    }

    const pubkeys = mentions.extractMentionPubkeys(trimmed);
    if (bountyAmountSats !== null) {
      try {
        resolveBountyTargetPubkey(pubkeys);
      } catch (error) {
        setSendError(
          error instanceof Error
            ? error.message
            : "Message bounties require exactly one @mention.",
        );
        return;
      }
    }

    // Send semantics use `undefined` for "no attachments" (no imeta tags
    // emitted on the publish), which is what `buildOutgoingMessage`
    // returns by default.
    const { content: finalContent, mediaTags } = buildOutgoingMessage(
      trimmed,
      currentPendingImeta,
    );

    const savedContent = trimmed;
    const savedImeta = [...currentPendingImeta];
    const savedKudos = isKudosActive;
    const savedBountyAmountSats = bountyAmountSats;

    setContent("");
    contentRef.current = "";
    richText.clearContent();
    media.setPendingImeta([]);
    mentions.clearMentions();
    channelLinks.clearChannels();
    emojiAutocomplete.clearEmojis();
    setIsKudosActive(false);
    setBountyAmountSats(null);
    setIsEmojiPickerOpen(false);

    const sentDraftKey = effectiveDraftKeyRef.current;
    setSendError(null);
    try {
      await onSendRef.current(finalContent, pubkeys, mediaTags, {
        bountyAmountSats: savedBountyAmountSats,
        kudos: savedKudos,
      });
      if (sentDraftKey) {
        drafts.clearDraft(sentDraftKey);
      }
    } catch (error) {
      setContent(savedContent);
      contentRef.current = savedContent;
      richText.setContent(savedContent);
      media.setPendingImeta(savedImeta);
      setIsKudosActive(savedKudos);
      setBountyAmountSats(savedBountyAmountSats);
      setSendError(
        error instanceof Error ? error.message : "Failed to send message.",
      );
    }
  }, [
    drafts.clearDraft,
    media.pendingImetaRef,
    media.setPendingImeta,
    mentions.extractMentionPubkeys,
    mentions.clearMentions,
    channelLinks.clearChannels,
    richText.clearContent,
    richText.setContent,
    emojiAutocomplete.clearEmojis,
    isKudosActive,
    bountyAmountSats,
    setIsKudosActive,
    setBountyAmountSats,
  ]);
  submitMessageRef.current = submitMessage;

  const handleSubmit = React.useCallback(
    (event: React.FormEvent<HTMLFormElement>) => {
      event.preventDefault();
      void submitMessage();
    },
    [submitMessage],
  );

  // ── Keyboard handling ───────────────────────────────────────────────
  // Tiptap handles formatting shortcuts (⌘B, ⌘I, etc.) natively.
  // Plain Enter → submit is now handled inside the Tiptap `submitOnEnter`
  // extension (fires before ProseMirror's splitBlock). This wrapper only
  // handles autocomplete arrow/enter keys and Escape for edit mode.
  const handleEditorKeyDown = React.useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      // Let autocomplete handle keys first
      const emojiResult = emojiAutocomplete.handleEmojiKeyDown(event);
      if (emojiResult.handled) {
        if (emojiResult.suggestion) {
          applyEmojiInsert(emojiResult.suggestion);
        }
        return;
      }

      const channelResult = channelLinks.handleChannelKeyDown(event);
      if (channelResult.handled) {
        if (channelResult.suggestion) {
          applyChannelInsert(channelResult.suggestion);
        }
        return;
      }

      const { handled, suggestion } = mentions.handleMentionKeyDown(event);
      if (handled) {
        if (suggestion) {
          applyMentionInsert(suggestion);
        }
        return;
      }

      // Escape in edit mode
      if (event.key === "Escape" && editTargetRef.current && onCancelEdit) {
        event.preventDefault();
        onCancelEdit();
        return;
      }
    },
    [
      emojiAutocomplete.handleEmojiKeyDown,
      applyEmojiInsert,
      channelLinks.handleChannelKeyDown,
      applyChannelInsert,
      mentions.handleMentionKeyDown,
      applyMentionInsert,
      onCancelEdit,
    ],
  );

  // ── Media paste + ⌘K link shortcut via Tiptap editorProps ──────────
  const uploadFileRef = React.useRef(media.uploadFile);
  uploadFileRef.current = media.uploadFile;

  React.useEffect(() => {
    if (!richText.editor) return;

    richText.editor.setOptions({
      editorProps: {
        ...richText.editor.options.editorProps,
        handlePaste: (_view, event) => {
          // --- Media paste ---
          const items = Array.from(event.clipboardData?.items ?? []);
          const mediaItem = items.find((item) =>
            ALLOWED_MEDIA_TYPES.includes(item.type),
          );
          if (mediaItem) {
            const file = mediaItem.getAsFile();
            if (file) {
              void uploadFileRef.current(file);
            }
            return true;
          }

          // --- Sprout code-block paste ---
          // The code block copy button writes a small Sprout marker alongside
          // plain text. Use it to paste back as a literal code block so Markdown
          // parsing cannot reshape indentation, fence markers, or headings.
          const codeBlockText = getSproutCodeBlockClipboardText(
            event.clipboardData,
          );
          if (codeBlockText !== null) {
            event.preventDefault();
            richText.editor
              ?.chain()
              .focus()
              .insertContent([
                {
                  type: "codeBlock",
                  content:
                    codeBlockText.length > 0
                      ? [{ type: "text", text: codeBlockText }]
                      : [],
                },
                { type: "paragraph" },
              ])
              .run();
            scrollComposerToBottom();
            return true;
          }

          // --- Mention / channel-link normalization ---
          // When copying from the chat area the browser puts styled HTML
          // on the clipboard. The mention/channel-link wrappers have
          // font-weight:600 which Tiptap's Bold extension misinterprets
          // as bold. Strip those wrappers and use ProseMirror's pasteHTML
          // to parse the cleaned HTML into proper rich content nodes.
          const html = event.clipboardData?.getData("text/html");
          if (html && hasMentionClipboardHtml(html)) {
            const cleanHtml = normalizeMentionClipboardHtml(html);
            event.preventDefault();
            _view.pasteHTML(cleanHtml);
            return true;
          }

          const plainText = event.clipboardData?.getData("text/plain") ?? "";
          if (plainText.includes("\n")) {
            scrollComposerToBottom();
          }

          return false;
        },
      },
    });
  }, [richText.editor, scrollComposerToBottom]);

  // ── Send button state ───────────────────────────────────────────────
  const sendDisabled = React.useMemo(
    () =>
      disabled ||
      media.isUploading ||
      (content.trim().length === 0 && media.pendingImeta.length === 0),
    [disabled, media.isUploading, content, media.pendingImeta.length],
  );

  const handleCaptureSelection = React.useCallback(() => {
    // No-op for Tiptap — selection is managed by ProseMirror.
  }, []);

  const handlePaperclipClick = React.useCallback(() => {
    void media.handlePaperclip();
  }, [media.handlePaperclip]);

  // ── Render ──────────────────────────────────────────────────────────
  return (
    <footer
      className={cn(
        "relative z-10 shrink-0 bg-transparent px-4 pb-2 pt-0",
        showTopBorder ? "border-t border-border/40 pt-3" : "",
        containerClassName,
      )}
    >
      <div
        aria-hidden="true"
        className="absolute inset-x-0 bottom-0 h-5 bg-background"
      />
      <div className="relative flex w-full flex-col gap-3">
        <form
          className="relative isolate rounded-2xl border border-border/50 bg-background/70 px-3 pb-2 pt-3 shadow-[0_4px_24px_rgba(0,0,0,0.08)] backdrop-blur-xl supports-[backdrop-filter]:bg-background/55 dark:shadow-[0_4px_24px_rgba(0,0,0,0.35)] sm:px-4"
          data-testid="message-composer"
          onDragEnter={media.handleDragEnter}
          onDragLeave={media.handleDragLeave}
          onDragOver={media.handleDragOver}
          onDrop={(e) => {
            void media.handleDrop(e);
          }}
          onSubmit={(event) => {
            handleSubmit(event);
          }}
        >
          {media.isDragOver && <DropZoneOverlay />}
          <EmojiAutocomplete
            onSelect={applyEmojiInsert}
            selectedIndex={emojiAutocomplete.emojiSelectedIndex}
            suggestions={
              emojiAutocomplete.isEmojiAutocompleteOpen
                ? emojiAutocomplete.emojiSuggestions
                : []
            }
          />
          <ChannelAutocomplete
            onSelect={applyChannelInsert}
            selectedIndex={channelLinks.channelSelectedIndex}
            suggestions={
              channelLinks.isChannelOpen ? channelLinks.channelSuggestions : []
            }
          />
          <MentionAutocomplete
            onSelect={applyMentionInsert}
            selectedIndex={mentions.mentionSelectedIndex}
            suggestions={mentions.isMentionOpen ? mentions.suggestions : []}
          />
          <ComposerContextBanner
            editTarget={editTarget}
            onCancelEdit={onCancelEdit}
            onCancelReply={onCancelReply}
            replyTarget={replyTarget}
          />

          {media.uploadState.status === "error" ? (
            <div className="mb-2 rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">
              Upload failed: {media.uploadState.message}
              <button
                className="ml-2 underline"
                onClick={() => media.setUploadState({ status: "idle" })}
                type="button"
              >
                Dismiss
              </button>
            </div>
          ) : null}

          {sendError ? (
            <div className="mb-2 rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">
              {sendError}
              <button
                className="ml-2 underline"
                onClick={() => setSendError(null)}
                type="button"
              >
                Dismiss
              </button>
            </div>
          ) : null}

          {isKudosActive ? (
            <ComposerKudosChip onRemove={() => setIsKudosActive(false)} />
          ) : null}
          {bountyAmountSats !== null ? (
            <ComposerBountyChip
              amountSats={bountyAmountSats}
              onRemove={() => setBountyAmountSats(null)}
            />
          ) : null}

          {(media.pendingImeta.length > 0 || media.isUploading) && (
            <div className="mb-2 flex items-center gap-2">
              <ComposerAttachments
                attachments={media.pendingImeta}
                isUploading={media.isUploading}
                uploadingCount={media.uploadingCount}
                onRemove={media.removeAttachment}
              />
            </div>
          )}

          {/* biome-ignore lint/a11y/noStaticElementInteractions: keydown handler bridges Tiptap editor to autocomplete and submit */}
          <div
            className="rich-text-composer max-h-32 overflow-y-auto"
            data-testid="message-input-scroll"
            ref={composerScrollRef}
            onKeyDown={handleEditorKeyDown}
          >
            <EditorContent editor={richText.editor} />
          </div>

          <MessageComposerToolbar
            composerDisabled={disabled}
            editor={richText.editor}
            extraActions={toolbarExtraActions}
            formattingDisabled={disabled}
            isEmojiPickerOpen={isEmojiPickerOpen}
            isFormattingOpen={isFormattingOpen}
            isBountyActive={bountyAmountSats !== null}
            isKudosActive={isKudosActive}
            isSending={isSending}
            isUploading={media.isUploading}
            kudosDisabled={kudosDisabled}
            bountyDisabled={bountyDisabled}
            onCaptureSelection={handleCaptureSelection}
            onAddBounty={() => {
              setIsEmojiPickerOpen(false);
              setIsFormattingOpen(false);
              handleAddBounty();
            }}
            onEmojiPickerOpenChange={setIsEmojiPickerOpen}
            onEmojiSelect={insertEmoji}
            onFormattingToggle={handleFormattingToggle}
            onGiveKudos={() => {
              setIsEmojiPickerOpen(false);
              setIsFormattingOpen(false);
              activateKudos();
            }}
            onOpenMentionPicker={openMentionPicker}
            onPaperclip={handlePaperclipClick}
            sendDisabled={sendDisabled}
          />
        </form>
        <ComposerBountyDialog
          amountValue={bountyDialog.amountValue}
          error={bountyDialog.error}
          isWalletLoading={bountyDialog.isWalletLoading}
          onAmountValueChange={bountyDialog.onAmountValueChange}
          onOpenChange={bountyDialog.onOpenChange}
          onSubmit={bountyDialog.onSubmit}
          open={bountyDialog.open}
          sendableBalanceSats={bountyDialog.sendableBalanceSats}
        />
      </div>
    </footer>
  );
}
